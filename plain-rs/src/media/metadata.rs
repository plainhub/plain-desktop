//! Media metadata probing (duration, artist, title).
//!
//! Probing is layered:
//! - ISO-BMFF containers (MP4/MOV/M4A/3GP) yield their duration from an
//!   in-process container walk (`moov → mvhd`, mirrors the Go `mp4duration`
//!   package): no track parsing, video-only files work — tag libraries like
//!   lofty reject those — and only the moov box is read, not the file.
//! - Everything else (duration of other containers, audio tags) is a single
//!   lofty parse.
//!
//! Callers cache results on the media row with (mtime, size) ref stamps
//! (see `media_scan::probe_missing_metadata`). The stamp is written for
//! every attempt — including probes that find nothing — so each file
//! version is probed at most once; a tagless file is not re-parsed on every
//! page view (the Go `Ensure*` helpers pay that cost forever).

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, TagType};
use std::path::Path;

/// Everything one probe pass can fill on a media row.
pub struct ProbedMeta {
    pub duration_secs: u32,
    pub artist: String,
    pub title: String,
    /// lofty parsed the file, so the fields above describe its actual
    /// content (empty tags on a parsed file mean "no tag"). When false the
    /// file could not be read at all — except that `duration_secs` may still
    /// carry a container-walk value; artist/title are *unknown*, not empty,
    /// and callers must keep their cached values.
    pub parsed: bool,
}

/// One probe pass for a media file: video reads the duration only (the
/// container walker first for the MP4 family); audio gets duration, artist
/// and title from a **single** lofty parse (with the container walker as
/// duration fallback when lofty cannot see one).
pub fn probe_media(path: &str, kind: &str) -> ProbedMeta {
    if kind != "audio" {
        return match probe_duration_secs(path) {
            Some(d) => ProbedMeta {
                duration_secs: d,
                artist: String::new(),
                title: String::new(),
                parsed: true,
            },
            None => ProbedMeta {
                duration_secs: 0,
                artist: String::new(),
                title: String::new(),
                parsed: false,
            },
        };
    }
    let ext = ext_of(path);
    match Probe::open(path).and_then(|p| p.read()) {
        Ok(f) => {
            let mut duration_secs = f.properties().duration().as_secs() as u32;
            if duration_secs == 0
                && is_mp4_family(&ext)
                && let Some(d) = mp4_duration_secs(path)
            {
                duration_secs = d;
            }
            let (artist, title) = tag_strings(&f);
            ProbedMeta {
                duration_secs,
                artist,
                title,
                parsed: true,
            }
        }
        Err(_) => {
            // Unreadable as far as lofty is concerned. The container walk
            // may still answer duration for the MP4 family; tags unknown.
            let duration_secs = if is_mp4_family(&ext) {
                mp4_duration_secs(path).unwrap_or(0)
            } else {
                0
            };
            ProbedMeta {
                duration_secs,
                artist: String::new(),
                title: String::new(),
                parsed: false,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

/// Duration (whole seconds) of a media file.
///
/// ISO-BMFF containers are parsed at the container level first; everything
/// else goes through lofty. `None` means nothing could read the file
/// (missing / unrecognized); `Some(0)` means it was read but carries no
/// computable duration. Callers keep cached values on `None`.
pub fn probe_duration_secs(path: &str) -> Option<u32> {
    if is_mp4_family(&ext_of(path)) {
        if let Some(d) = mp4_duration_secs(path) {
            return Some(d);
        }
    }
    let file = Probe::open(path).and_then(|p| p.read()).ok()?;
    Some(file.properties().duration().as_secs() as u32)
}

fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn is_mp4_family(ext: &str) -> bool {
    matches!(ext, "mp4" | "m4a" | "mov" | "m4v" | "3gp" | "3gpp")
}

/// Whole-second duration of an ISO-BMFF file: walk top-level boxes to
/// `moov` (head or tail), buffer only the moov payload, then scan its
/// children for `mvhd` and convert `duration / timescale`. `None` on any
/// malformation — the caller falls back to the tag probe.
fn mp4_duration_secs(path: &str) -> Option<u32> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut pos: u64 = 0;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut hdr = [0u8; 8];
        f.read_exact(&mut hdr).ok()?;
        let size = u32::from_be_bytes(hdr[0..4].try_into().ok()?) as u64;
        let (box_len, header_len) = match size {
            1 => {
                let mut ext = [0u8; 8];
                f.read_exact(&mut ext).ok()?;
                (u64::from_be_bytes(ext), 16)
            }
            // 0 = box extends to EOF; other sub-8 sizes are malformed and
            // rejected by the `box_len < header_len` check below.
            0 => (len - pos, 8),
            s => (s, 8),
        };
        if box_len < header_len || pos + box_len > len {
            return None;
        }
        if &hdr[4..8] == b"moov" {
            let payload = box_len - header_len;
            if payload > MAX_MOOV_BYTES {
                return None;
            }
            let mut moov = vec![0u8; payload as usize];
            f.read_exact(&mut moov).ok()?;
            return mvhd_duration_secs(&moov);
        }
        pos += box_len;
    }
    None
}

/// Scan moov child boxes for `mvhd` and read `duration / timescale` as
/// whole seconds (version 0: 32-bit fields, version 1: 64-bit).
fn mvhd_duration_secs(moov: &[u8]) -> Option<u32> {
    let mut off = 0usize;
    while off + 8 <= moov.len() {
        let size = u32::from_be_bytes(moov[off..off + 4].try_into().ok()?) as usize;
        if size < 8 || off + size > moov.len() {
            return None;
        }
        if &moov[off + 4..off + 8] == b"mvhd" {
            return mvhd_box_duration(&moov[off + 8..off + size]);
        }
        off += size;
    }
    None
}

/// duration/timescale from one mvhd payload (everything past the box
/// header). A payloadless box is malformed → `None`, never a panic.
fn mvhd_box_duration(b: &[u8]) -> Option<u32> {
    let (timescale, duration_ticks) = if b.first() == Some(&1) {
        // version+flags (4) + creation (8) + modification (8)
        if b.len() < 32 {
            return None;
        }
        (
            u32::from_be_bytes(b[20..24].try_into().ok()?),
            u64::from_be_bytes(b[24..32].try_into().ok()?),
        )
    } else {
        // version+flags (4) + creation (4) + modification (4)
        if b.len() < 20 {
            return None;
        }
        (
            u32::from_be_bytes(b[12..16].try_into().ok()?),
            u64::from(u32::from_be_bytes(b[16..20].try_into().ok()?)),
        )
    };
    if timescale == 0 {
        return None;
    }
    Some((duration_ticks / u64::from(timescale)) as u32)
}

/// Guard for the buffered moov payload (same order as the codec walker).
const MAX_MOOV_BYTES: u64 = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Tag strings
// ---------------------------------------------------------------------------

/// First non-empty artist and title across the tag types plain-app reads,
/// with the primary tag as a fallback.
fn tag_strings(tagged_file: &lofty::file::TaggedFile) -> (String, String) {
    let mut artist = String::new();
    let mut title = String::new();
    for tag_type in [
        TagType::Id3v2,
        TagType::Ape,
        TagType::Id3v1,
        TagType::VorbisComments,
        TagType::Mp4Ilst,
    ] {
        if let Some(tag) = tagged_file.tag(tag_type) {
            if artist.is_empty()
                && let Some(a) = tag.artist()
                && !a.trim().is_empty()
            {
                artist = a.trim().to_string();
            }
            if title.is_empty()
                && let Some(t) = tag.title()
                && !t.trim().is_empty()
            {
                title = t.trim().to_string();
            }
        }
    }
    if artist.is_empty() || title.is_empty() {
        if let Some(tag) = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())
        {
            if artist.is_empty()
                && let Some(a) = tag.artist()
                && !a.trim().is_empty()
            {
                artist = a.trim().to_string();
            }
            if title.is_empty()
                && let Some(t) = tag.title()
                && !t.trim().is_empty()
            {
                title = t.trim().to_string();
            }
        }
    }
    (artist, title)
}

#[cfg(test)]
#[path = "../../tests/unit/media/metadata.rs"]
mod tests;
