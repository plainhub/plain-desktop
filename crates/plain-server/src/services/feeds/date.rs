use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};

/// Parses the date formats feed publishers actually emit.
///
/// `parse_from_rfc3339` / `parse_from_rfc2822` come first, then two lenient
/// paths, because the strict parsers reject dates that are perfectly readable
/// and that the app used to understand: seconds are optional in RFC 822,
/// two-digit years are common, and plenty of feeds leave the zone off or write
/// `UT`. A date we fail to read is not a small mistake — the entry silently
/// falls back to "published now" and resurfaces at the top of the list on
/// every sync.
pub(super) fn parse(text: &str) -> Option<DateTime<FixedOffset>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .or_else(|| DateTime::parse_from_rfc2822(text).ok())
        .or_else(|| parse_unzoned(text))
        .or_else(|| parse_rfc822(text))
}

/// A W3C date-time whose zone the publisher forgot (`2024-09-10 12:00:00`,
/// `2024-09-10T12:00`, `2024-09-10`). Read as UTC, which is what an unzoned
/// timestamp means everywhere it is emitted in practice.
fn parse_unzoned(text: &str) -> Option<DateTime<FixedOffset>> {
    let (date_part, time_part) = match text.find(['T', 't', ' ']) {
        Some(index) => (&text[..index], text[index + 1..].trim()),
        None => (text, ""),
    };
    let mut date = date_part.split('-');
    let year: i32 = date.next()?.parse().ok()?;
    let month: u32 = date.next().unwrap_or("1").parse().ok()?;
    let day: u32 = date.next().unwrap_or("1").parse().ok()?;
    if date.next().is_some() {
        return None;
    }
    let (hour, minute, second) = if time_part.is_empty() {
        (0, 0, 0)
    } else {
        // Both `,` and `.` introduce fractional seconds in the wild.
        let mut clock = time_part.split(['.', ',']).next()?.split(':');
        (
            clock.next()?.parse().ok()?,
            clock.next().unwrap_or("0").parse().ok()?,
            clock.next().unwrap_or("0").parse().ok()?,
        )
    };
    build(year, month, day, hour, minute, second, 0)
}

/// RFC 822/2822 the way publishers write it: optional day name, optional
/// seconds, two- or four-digit years, and zone names RFC 2822 dropped.
fn parse_rfc822(text: &str) -> Option<DateTime<FixedOffset>> {
    let body = strip_day_name(text);
    let mut parts = body.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let year = expand_year(parts.next()?.parse::<i32>().ok()?);
    let mut clock = parts.next()?.split(':');
    let hour: u32 = clock.next()?.parse().ok()?;
    let minute: u32 = clock.next().unwrap_or("0").parse().ok()?;
    let second: u32 = clock.next().unwrap_or("0").parse().ok()?;
    build(
        year,
        month,
        day,
        hour,
        minute,
        second,
        parts.next().map_or(0, zone_offset),
    )
}

fn strip_day_name(text: &str) -> &str {
    if let Some(comma) = text.find(',') {
        if (1..=4).contains(&comma) && text[..comma].chars().all(char::is_alphabetic) {
            return text[comma + 1..].trim_start();
        }
    }
    let mut parts = text.split_whitespace();
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    if !first.is_empty()
        && first.chars().count() <= 4
        && first.chars().all(char::is_alphabetic)
        && second.starts_with(|c: char| c.is_ascii_digit())
    {
        return second;
    }
    text
}

fn expand_year(year: i32) -> i32 {
    if year >= 100 {
        year
    } else if year < 50 {
        2000 + year
    } else {
        1900 + year
    }
}

fn month_number(name: &str) -> Option<u32> {
    Some(match name.get(..3)?.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

fn zone_offset(zone: &str) -> i32 {
    let zone = zone.trim();
    if let Some(offset) = numeric_offset(zone) {
        return offset;
    }
    match zone.to_ascii_lowercase().as_str() {
        "ut" | "utc" | "gmt" | "z" => 0,
        "est" => -5 * 3600,
        "edt" => -4 * 3600,
        "cst" => -6 * 3600,
        "cdt" => -5 * 3600,
        "mst" => -7 * 3600,
        "mdt" => -6 * 3600,
        "pst" => -8 * 3600,
        "pdt" => -7 * 3600,
        // An unreadable zone still leaves a usable timestamp; guessing UTC
        // beats dropping the date and calling the entry brand new.
        _ => 0,
    }
}

fn numeric_offset(zone: &str) -> Option<i32> {
    let (sign, digits) = match zone.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, zone.strip_prefix('+')?),
    };
    let digits: String = digits.chars().filter(|c| *c != ':').collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hours = digits
        .get(..2)
        .and_then(|h| h.parse::<i32>().ok())
        .unwrap_or(0);
    let minutes = digits
        .get(2..4)
        .and_then(|m| m.parse::<i32>().ok())
        .unwrap_or(0);
    Some(sign * (hours * 3600 + minutes * 60))
}

fn build(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    offset: i32,
) -> Option<DateTime<FixedOffset>> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
    FixedOffset::east_opt(offset)?
        .from_local_datetime(&date)
        .single()
}
