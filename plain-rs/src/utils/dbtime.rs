//! Timestamp helpers for database rows (no chrono — std only).
//!
//! Two precisions, both fixed-width so lexicographic comparison equals
//! chronological comparison within a column:
//! - `now_iso` / `unix_secs_to_iso8601`: whole seconds — the chat tables'
//!   format, shared with plain-app's Room rows.
//! - `now_iso_millis`: always three fraction digits — the library tables
//!   (playlists, play history), where same-second rows must still order.

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    unix_secs_to_iso8601(secs)
}

pub fn now_iso_millis() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    unix_millis_to_iso8601(dur.as_millis() as u64)
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Second-precision rendering (the chat tables' format): sub-second
/// digits are truncated, never rendered.
pub fn iso_from_unix_millis(millis: i64) -> String {
    unix_secs_to_iso8601(millis.max(0) as u64 / 1000)
}

fn is_leap(y: u64) -> bool {
    y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400))
}

fn unix_secs_to_iso8601(secs: u64) -> String {
    let (date, hh, mm, ss) = split_unix_secs(secs);
    format!("{date}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn unix_millis_to_iso8601(millis: u64) -> String {
    let (date, hh, mm, ss) = split_unix_secs(millis / 1000);
    let ms = millis % 1000;
    format!("{date}T{hh:02}:{mm:02}:{ss:02}.{ms:03}Z")
}

/// Civil-date split of a unix timestamp: (YYYY-MM-DD, hh, mm, ss).
fn split_unix_secs(secs: u64) -> (String, u64, u64, u64) {
    let ss = secs % 60;
    let t = secs / 60;
    let mm = t % 60;
    let t = t / 60;
    let hh = t % 24;
    let mut days = t / 24;
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366u64 } else { 365u64 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let md: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dm in md.iter() {
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    (
        format!("{:04}-{:02}-{:02}", year, month, days + 1),
        hh,
        mm,
        ss,
    )
}
