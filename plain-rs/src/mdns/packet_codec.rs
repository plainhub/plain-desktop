//! DNS/mDNS wire-format codec shared by the hostname responder, the service
//! publisher and the service browser.
//!
//! Covers:
//!  - A-record query matching / response building (hostname responder)
//!  - PTR/SRV/TXT/A query building (browser)
//!  - DNS message parsing into typed records (browser)

use super::service_info::{MdnsParsedResponse, MdnsRecord};

pub const DNS_CLASS_IN: u16 = 0x0001;
pub const TYPE_A: u16 = 0x0001;
pub const TYPE_AAAA: u16 = 0x001C;
pub const TYPE_PTR: u16 = 0x000C;
pub const TYPE_TXT: u16 = 0x0010;
pub const TYPE_SRV: u16 = 0x0021;
pub const TYPE_ANY: u16 = 0x00FF;
pub const DNS_RESPONSE_FLAGS: u16 = 0x8400;
pub const DNS_CACHE_FLUSH_CLASS_IN: u16 = 0x8001;
pub const TTL_SECONDS: u32 = 120;

#[derive(Debug, Clone, PartialEq)]
pub struct MdnsQuestion {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
    pub unicast_response_requested: bool,
}

#[derive(Debug, Clone)]
pub struct MdnsResponse {
    pub bytes: Vec<u8>,
    pub matched_questions: Vec<MdnsQuestion>,
}

impl MdnsResponse {
    pub fn unicast_response_requested(&self) -> bool {
        self.matched_questions
            .iter()
            .any(|q| q.unicast_response_requested)
    }
}

pub fn build_response_if_match(
    query: &[u8],
    hostname: &str,
    ips: &[String],
) -> Option<MdnsResponse> {
    if ips.is_empty() {
        return None;
    }
    let questions = read_questions(query)?;
    let matched_questions: Vec<MdnsQuestion> = questions
        .iter()
        .filter(|q| {
            q.name.eq_ignore_ascii_case(hostname)
                && q.qclass == DNS_CLASS_IN
                && (q.qtype == TYPE_A || q.qtype == TYPE_ANY)
        })
        .cloned()
        .collect();
    if matched_questions.is_empty() {
        return None;
    }

    let name_bytes = encode_name(hostname);
    let mut out = Vec::new();
    write_header(&mut out, ips.len(), 0);
    for ip in ips {
        write_record(
            &mut out,
            &name_bytes,
            TYPE_A,
            DNS_CACHE_FLUSH_CLASS_IN,
            TTL_SECONDS,
            &ip_to_bytes(ip),
        );
    }
    Some(MdnsResponse {
        bytes: out,
        matched_questions,
    })
}

// ---- Query builders ---------------------------------------------------------

pub fn build_query(name: &str, qtype: u16, unicast_response: bool) -> Vec<u8> {
    let mut out = Vec::new();
    write_u16(&mut out, 0); // ID
    write_u16(&mut out, 0); // flags: query
    write_u16(&mut out, 1); // QDCOUNT
    write_u16(&mut out, 0); // ANCOUNT
    write_u16(&mut out, 0); // NSCOUNT
    write_u16(&mut out, 0); // ARCOUNT
    out.extend_from_slice(&encode_name(name));
    write_u16(&mut out, qtype);
    write_u16(
        &mut out,
        if unicast_response {
            0x8001
        } else {
            DNS_CLASS_IN
        },
    );
    out
}

pub fn build_ptr_query(service_type: &str) -> Vec<u8> {
    build_query(service_type, TYPE_PTR, false)
}

pub fn build_srv_query(instance_name: &str, service_type: &str) -> Vec<u8> {
    build_query(&format!("{instance_name}.{service_type}"), TYPE_SRV, false)
}

pub fn build_txt_query(instance_name: &str, service_type: &str) -> Vec<u8> {
    build_query(&format!("{instance_name}.{service_type}"), TYPE_TXT, false)
}

// ---- Response parsing ---------------------------------------------------------

/// Parses a DNS/mDNS message into its answers and additional records.
/// The query section (if present) is skipped.
pub fn parse_response(data: &[u8]) -> Option<MdnsParsedResponse> {
    if data.len() < 12 {
        return None;
    }
    let flags = read_u16(data, 2);
    let qd_count = read_u16(data, 4) as usize;
    let an_count = read_u16(data, 6) as usize;
    let ns_count = read_u16(data, 8) as usize;
    let ar_count = read_u16(data, 10) as usize;

    let mut offset = 12usize;
    for _ in 0..qd_count {
        let (_, next) = read_name(data, offset, 0)?;
        offset = next + 4;
        if offset > data.len() {
            return None;
        }
    }
    let (answers, offset) = read_records(data, offset, an_count)?;
    let (_authority, offset) = read_records(data, offset, ns_count)?;
    let (additional, _) = read_records(data, offset, ar_count)?;
    Some(MdnsParsedResponse {
        flags,
        answers,
        additional,
    })
}

fn read_records(data: &[u8], start: usize, count: usize) -> Option<(Vec<MdnsRecord>, usize)> {
    let mut records = Vec::with_capacity(count);
    let mut offset = start;
    for _ in 0..count {
        let (name, next) = read_name(data, offset, 0)?;
        offset = next;
        if offset + 10 > data.len() {
            return None;
        }
        let record_type = read_u16(data, offset);
        let ttl = read_u32(data, offset + 4);
        let rdlen = read_u16(data, offset + 8) as usize;
        offset += 10;
        if offset + rdlen > data.len() {
            return None;
        }
        records.push(MdnsRecord {
            name,
            record_type,
            ttl,
            packet: data.to_vec(),
            rdata_start: offset,
            rdata_length: rdlen,
        });
        offset += rdlen;
    }
    Some((records, offset))
}

/// Parses the question section of a query message. None if not a query.
pub fn read_questions(data: &[u8]) -> Option<Vec<MdnsQuestion>> {
    if data.len() < 12 {
        return None;
    }
    // Bit 15 (QR) = 1 means this is a response, not a query. Ignore it.
    if read_u16(data, 2) & 0x8000 != 0 {
        return None;
    }
    let qd_count = read_u16(data, 4) as usize;
    if qd_count == 0 {
        return None;
    }

    let mut offset = 12usize;
    let mut questions = Vec::with_capacity(qd_count);
    for _ in 0..qd_count {
        let (qname, next) = read_name(data, offset, 0)?;
        offset = next;
        if offset + 4 > data.len() {
            return None;
        }
        let qtype = read_u16(data, offset);
        let qclass_raw = read_u16(data, offset + 2);
        questions.push(MdnsQuestion {
            name: qname,
            qtype,
            qclass: qclass_raw & 0x7FFF,
            unicast_response_requested: qclass_raw & 0x8000 != 0,
        });
        offset += 4;
    }
    Some(questions)
}

// ---- DNS wire-format helpers ---------------------------------------------------

pub fn write_header(out: &mut Vec<u8>, answers: usize, additional: usize) {
    write_u16(out, 0);
    write_u16(out, DNS_RESPONSE_FLAGS);
    write_u16(out, 0);
    write_u16(out, answers as u16);
    write_u16(out, 0);
    write_u16(out, additional as u16);
}

pub fn write_record(
    out: &mut Vec<u8>,
    name: &[u8],
    record_type: u16,
    cls: u16,
    ttl: u32,
    rdata: &[u8],
) {
    out.extend_from_slice(name);
    write_u16(out, record_type);
    write_u16(out, cls);
    write_u32(out, ttl);
    write_u16(out, rdata.len() as u16);
    out.extend_from_slice(rdata);
}

pub fn encode_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.split('.').filter(|l| !l.is_empty()) {
        let bytes = label.as_bytes();
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
    }
    out.push(0);
    out
}

pub fn read_name(data: &[u8], start: usize, depth: u32) -> Option<(String, usize)> {
    if depth > 8 || start >= data.len() {
        return None;
    }

    let mut labels: Vec<String> = Vec::new();
    let mut offset = start;
    while offset < data.len() {
        let len = data[offset] as usize;
        if len == 0 {
            return Some((labels.join("."), offset + 1));
        }

        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= data.len() {
                return None;
            }
            let ptr = ((len & 0x3F) << 8) | data[offset + 1] as usize;
            let (pointed, _) = read_name(data, ptr, depth + 1)?;
            let pointed_labels: Vec<String> = pointed
                .split('.')
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            labels.extend(pointed_labels);
            return Some((labels.join("."), offset + 2));
        }

        let next = offset + 1 + len;
        if next > data.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&data[offset + 1..offset + 1 + len]).to_string());
        offset = next;
    }
    None
}

pub fn read_u16(data: &[u8], offset: usize) -> u16 {
    ((data[offset] as u16) << 8) | data[offset + 1] as u16
}

pub fn read_u32(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32) << 24)
        | ((data[offset + 1] as u32) << 16)
        | ((data[offset + 2] as u32) << 8)
        | (data[offset + 3] as u32)
}

pub fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.push((value >> 8) as u8);
    out.push((value & 0xFF) as u8);
}

pub fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.push((value >> 24) as u8);
    out.push((value >> 16) as u8);
    out.push((value >> 8) as u8);
    out.push((value & 0xFF) as u8);
}

pub fn ip_to_bytes(ip: &str) -> Vec<u8> {
    ip.split('.')
        .filter_map(|part| part.parse::<u8>().ok())
        .collect()
}

/// Formats 16 raw bytes of AAAA RDATA as standard IPv6 text, compressing the
/// longest run of zero hextets with "::" (RFC 5952).
pub fn ipv6_to_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 {
        return None;
    }
    let hextets: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| ((c[0] as u16) << 8) | c[1] as u16)
        .collect();
    let mut best: Option<(usize, usize)> = None; // (start, len) of longest zero run
    let mut i = 0;
    while i < hextets.len() {
        if hextets[i] == 0 {
            let start = i;
            while i < hextets.len() && hextets[i] == 0 {
                i += 1;
            }
            let len = i - start;
            if best.map_or(true, |(_, bl)| len > bl) {
                best = Some((start, len));
            }
        } else {
            i += 1;
        }
    }
    let fmt = |r: &[u16]| {
        r.iter()
            .map(|h| format!("{h:x}"))
            .collect::<Vec<_>>()
            .join(":")
    };
    match best {
        Some((s, l)) if l >= 2 => {
            let e = s + l;
            let left = &hextets[..s];
            let right = &hextets[e..];
            Some(match (left.is_empty(), right.is_empty()) {
                (true, true) => "::".to_string(),
                (true, false) => format!("::{}", fmt(right)),
                (false, true) => format!("{}::", fmt(left)),
                (false, false) => format!("{}::{}", fmt(left), fmt(right)),
            })
        }
        _ => Some(fmt(&hextets)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response_skips_authority_section() {
        let mut out = Vec::new();
        write_u16(&mut out, 0);
        write_u16(&mut out, DNS_RESPONSE_FLAGS);
        write_u16(&mut out, 0);
        write_u16(&mut out, 1);
        write_u16(&mut out, 1);
        write_u16(&mut out, 1);
        write_record(
            &mut out,
            &encode_name("_plainapp._tcp.local"),
            TYPE_PTR,
            DNS_CLASS_IN,
            TTL_SECONDS,
            &encode_name("X._plainapp._tcp.local"),
        );
        write_record(
            &mut out,
            &encode_name("junk.local"),
            TYPE_A,
            DNS_CLASS_IN,
            TTL_SECONDS,
            &[1, 2, 3, 4],
        );
        write_record(
            &mut out,
            &encode_name("p9.local"),
            TYPE_A,
            DNS_CACHE_FLUSH_CLASS_IN,
            TTL_SECONDS,
            &[192, 168, 1, 20],
        );

        let parsed = parse_response(&out).expect("parse");
        assert_eq!(parsed.answers.len(), 1);
        assert_eq!(parsed.additional.len(), 1);
        assert_eq!(parsed.additional[0].name, "p9.local");
        assert_eq!(parsed.additional[0].ip().unwrap(), "192.168.1.20");
    }

    #[test]
    fn ipv6_to_string_compresses_longest_zero_run() {
        let v = |vals: &[u16]| {
            let mut b = Vec::new();
            for x in vals {
                b.push((x >> 8) as u8);
                b.push((x & 0xFF) as u8);
            }
            ipv6_to_string(&b).expect("ipv6")
        };
        assert_eq!(
            v(&[0xfe80, 0, 0, 0, 0, 0xcc7, 0x94ea, 0xcb1]),
            "fe80::cc7:94ea:cb1"
        );
        assert_eq!(v(&[0x2001, 0xdb8, 0, 0, 0, 0, 0, 1]), "2001:db8::1");
        assert_eq!(v(&[0, 0, 0, 0, 0, 0, 0, 1]), "::1");
        assert_eq!(v(&[1, 2, 3, 4, 5, 6, 7, 8]), "1:2:3:4:5:6:7:8");
        assert!(ipv6_to_string(&[1, 2, 3, 4]).is_none());
    }

    #[test]
    fn build_query_qu_sets_unicast_response_bit() {
        let qu = build_query("_plainapp._tcp.local", TYPE_PTR, true);
        let question = &read_questions(&qu).expect("questions")[0];
        assert_eq!(question.qclass, DNS_CLASS_IN);
        assert!(question.unicast_response_requested);

        let qm = build_query("_plainapp._tcp.local", TYPE_PTR, false);
        assert!(!read_questions(&qm).expect("questions")[0].unicast_response_requested);
    }

    #[test]
    fn parse_response_tolerates_question_section() {
        // Legacy-unicast (QU) responses echo the question before the answers
        // (RFC 6762 §6.7); the browser must still see the records.
        let mut out = Vec::new();
        write_u16(&mut out, 0);
        write_u16(&mut out, DNS_RESPONSE_FLAGS);
        write_u16(&mut out, 1);
        write_u16(&mut out, 1);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        out.extend_from_slice(&encode_name("_plainapp._tcp.local"));
        write_u16(&mut out, TYPE_PTR);
        write_u16(&mut out, DNS_CLASS_IN);
        write_record(
            &mut out,
            &encode_name("_plainapp._tcp.local"),
            TYPE_PTR,
            DNS_CLASS_IN,
            TTL_SECONDS,
            &encode_name("Pixel 7._plainapp._tcp.local"),
        );

        let parsed = parse_response(&out).expect("parse");
        assert!(parsed.is_response());
        assert_eq!(parsed.answers.len(), 1);
        assert_eq!(
            parsed.answers[0].ptr_target().unwrap(),
            "Pixel 7._plainapp._tcp.local"
        );
    }
}
