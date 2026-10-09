mod gateway;
use anyhow::{Result, bail, ensure};
pub use gateway::GatewayRequest;
use std::time::{Duration, Instant};

pub const HEADER: usize = 10;
pub const MAX_BODY: usize = 4 * 1024 * 1024;
pub const MAX_MESSAGE: usize = 8 + 65536 + MAX_BODY;

pub fn message(metadata: &[u8], body: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        metadata.len() <= 65536 && body.len() <= MAX_BODY,
        "BLE message exceeds limit"
    );
    let mut out = Vec::with_capacity(8 + metadata.len() + body.len());
    out.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(metadata);
    out.extend_from_slice(body);
    Ok(out)
}
pub fn parts(data: &[u8]) -> Result<(&[u8], &[u8])> {
    ensure!(data.len() >= 8, "Missing BLE message prefix");
    let m = u32::from_le_bytes(data[..4].try_into()?) as usize;
    let b = u32::from_le_bytes(data[4..8].try_into()?) as usize;
    ensure!(
        m <= 65536 && b <= MAX_BODY && data.len() == 8 + m + b,
        "Invalid BLE message length"
    );
    Ok((&data[8..8 + m], &data[8 + m..]))
}
pub fn nearby_body(data: &[u8]) -> Result<&[u8]> {
    let (metadata, body) = parts(data)?;
    ensure!(metadata.is_empty(), "Invalid Nearby metadata");
    Ok(body)
}
fn string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    ensure!(value.len() <= u16::MAX as usize, "BLE string exceeds limit");
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(self.0.len() >= n, "Truncated BLE metadata");
        let (data, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(data)
    }
    fn string(&mut self) -> Result<String> {
        let n = u16::from_le_bytes(self.take(2)?.try_into()?) as usize;
        Ok(std::str::from_utf8(self.take(n)?)?.to_owned())
    }
}
#[derive(Debug, PartialEq)]
pub enum Request {
    PeerGraphql {
        client_id: String,
        channel_id: String,
        body: Vec<u8>,
    },
    FileChunk {
        client_id: String,
        file_id: String,
        offset: u64,
        length: u32,
    },
}
impl Request {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut m = Vec::new();
        let body = match self {
            Self::PeerGraphql {
                client_id,
                channel_id,
                body,
            } => {
                ensure!(!client_id.is_empty(), "Missing BLE client ID");
                m.push(1);
                string(&mut m, client_id)?;
                string(&mut m, channel_id)?;
                body.as_slice()
            }
            Self::FileChunk {
                client_id,
                file_id,
                offset,
                length,
            } => {
                ensure!(
                    !client_id.is_empty() && !file_id.is_empty(),
                    "Missing BLE file identity"
                );
                ensure!(
                    (1..=8192).contains(length) && offset.checked_add(*length as u64).is_some(),
                    "Invalid BLE file range"
                );
                m.push(2);
                string(&mut m, client_id)?;
                string(&mut m, file_id)?;
                m.extend_from_slice(&offset.to_le_bytes());
                m.extend_from_slice(&length.to_le_bytes());
                &[]
            }
        };
        message(&m, body)
    }
    pub fn decode(data: &[u8]) -> Result<Self> {
        let (metadata, body) = parts(data)?;
        let mut r = Reader(metadata);
        let operation = r.take(1)?[0];
        let client_id = r.string()?;
        ensure!(!client_id.is_empty(), "Missing BLE client ID");
        let result = match operation {
            1 => Self::PeerGraphql {
                client_id,
                channel_id: r.string()?,
                body: body.to_vec(),
            },
            2 => {
                let file_id = r.string()?;
                let offset = u64::from_le_bytes(r.take(8)?.try_into()?);
                let length = u32::from_le_bytes(r.take(4)?.try_into()?);
                ensure!(
                    !file_id.is_empty()
                        && body.is_empty()
                        && (1..=8192).contains(&length)
                        && offset.checked_add(length as u64).is_some(),
                    "Invalid BLE file request"
                );
                Self::FileChunk {
                    client_id,
                    file_id,
                    offset,
                    length,
                }
            }
            _ => bail!("Unknown BLE operation"),
        };
        ensure!(r.0.is_empty(), "Trailing BLE metadata");
        Ok(result)
    }
}
pub fn response(status: u16, body: &[u8]) -> Result<Vec<u8>> {
    ensure!((100..600).contains(&status), "Invalid BLE HTTP status");
    message(&status.to_le_bytes(), body)
}
pub fn decode_response(data: &[u8]) -> Result<(u16, &[u8])> {
    let (m, b) = parts(data)?;
    ensure!(m.len() == 2, "Invalid BLE response metadata");
    let status = u16::from_le_bytes(m.try_into()?);
    ensure!((100..600).contains(&status), "Invalid BLE HTTP status");
    Ok((status, b))
}
pub fn frame(
    data: &[u8],
    id: u32,
    response: bool,
    sequence: u32,
    limit: usize,
) -> Result<Option<Vec<u8>>> {
    parts(data)?;
    ensure!(
        id != 0 && limit > HEADER && limit <= 512,
        "Invalid BLE frame parameters"
    );
    let size = limit - HEADER;
    let start = (sequence as usize)
        .checked_mul(size)
        .ok_or_else(|| anyhow::anyhow!("BLE sequence overflow"))?;
    if start >= data.len() {
        return Ok(None);
    }
    let end = start.saturating_add(size).min(data.len());
    let mut out = Vec::with_capacity(HEADER + end - start);
    out.push(1);
    out.push(u8::from(start == 0) | (u8::from(end == data.len()) << 1) | (u8::from(response) << 2));
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&data[start..end]);
    Ok(Some(out))
}
#[derive(Default)]
pub struct Assembler {
    id: u32,
    response: bool,
    next: u32,
    data: Vec<u8>,
    last: Option<Instant>,
    first: Option<Instant>,
}
impl Assembler {
    pub fn info(&self) -> u64 {
        self.id as u64 | ((self.response as u64) << 32)
    }
    pub fn push(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>> {
        let result = self.push_inner(frame);
        if result.is_err() {
            self.data.clear();
            self.last = None;
            self.first = None;
            self.next = 0;
        }
        result
    }
    fn push_inner(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>> {
        ensure!(
            frame.len() >= HEADER && frame.len() <= 512 && frame[0] == 1 && frame[1] & !7 == 0,
            "Invalid BLE frame"
        );
        let id = u32::from_le_bytes(frame[2..6].try_into()?);
        let sequence = u32::from_le_bytes(frame[6..10].try_into()?);
        let response = frame[1] & 4 != 0;
        ensure!(
            id != 0 && (frame[1] & 1 != 0) == (sequence == 0),
            "Invalid BLE START"
        );
        if let Some(last) = self.last {
            ensure!(
                self.first
                    .is_some_and(|first| first.elapsed() < Duration::from_secs(120)),
                "BLE message deadline exceeded"
            );
            ensure!(
                last.elapsed() < Duration::from_secs(15),
                "BLE assembly timed out"
            );
            ensure!(
                sequence == self.next && id == self.id && response == self.response,
                "Unexpected BLE fragment"
            );
        } else {
            ensure!(sequence == 0, "Missing BLE START");
            self.id = id;
            self.response = response;
            self.first = Some(Instant::now());
        }
        ensure!(
            self.data.len() + frame.len() - HEADER <= MAX_MESSAGE,
            "BLE assembly exceeds limit"
        );
        self.data.extend_from_slice(&frame[HEADER..]);
        if self.data.len() >= 8 {
            let m = u32::from_le_bytes(self.data[..4].try_into()?) as usize;
            let b = u32::from_le_bytes(self.data[4..8].try_into()?) as usize;
            ensure!(
                m <= 65536 && b <= MAX_BODY && self.data.len() <= 8 + m + b,
                "Invalid BLE declared length"
            );
            ensure!(
                frame[1] & 2 != 0 || self.data.len() < 8 + m + b,
                "Missing BLE END"
            );
        }
        self.next = sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("BLE sequence overflow"))?;
        self.last = Some(Instant::now());
        if frame[1] & 2 == 0 {
            return Ok(None);
        }
        parts(&self.data)?;
        self.last = None;
        self.first = None;
        self.next = 0;
        Ok(Some(std::mem::take(&mut self.data)))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ble/mod.rs"]
mod tests;

#[cfg(feature = "ble-native")]
pub mod ffi;
