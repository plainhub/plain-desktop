//! Stable media UUID generation from filesystem identity.
//!
//! 1:1 port of Go's `internal/media/uuid.go`. A media item's UUID is
//! derived from the triplet (FSUUID, inode, ctime) so it survives path
//! renames. FSUUID is resolved from `/dev/disk/by-uuid` + `/proc/mounts`
//! with a 30-second TTL cache.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sha1::{Digest, Sha1};

/// Namespace bytes used as the SHA-1 prefix (same as Go's `uuidNamespace`).
const UUID_NAMESPACE: [u8; 16] = [
    0x6d, 0x65, 0x64, 0x69, 0x61, 0x2d, 0x6e, 0x61, 0x73, 0x2d, 0x69, 0x64, 0x00, 0x00, 0x00, 0x00,
];

/// Key prefix of the (fsuuid, inode, ctime) → uuid secondary index.
pub const FID_INDEX_PREFIX: &str = "media:fid:";

/// (mountpoint with trailing slash, filesystem id), longest mountpoint first.
/// The slash is pre-baked so the per-file hot path is a plain `starts_with`
/// with no allocation.
type MountTable = Vec<(String, String)>;

struct MountCache {
    table: MountTable,
    fsid_of_root: String,
    built_at: std::time::Instant,
}

static CACHE: Mutex<Option<Arc<MountCache>>> = Mutex::new(None);
const TTL: std::time::Duration = std::time::Duration::from_secs(30);

// ---------------------------------------------------------------------------
// /proc/mounts octal-escape decoder
// ---------------------------------------------------------------------------

fn decode_fstab_escapes(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

// ---------------------------------------------------------------------------
// /dev/disk/by-uuid → resolved dev path
// ---------------------------------------------------------------------------

fn build_dev_to_uuid_map() -> HashMap<PathBuf, String> {
    let mut m = HashMap::new();
    let dir = match std::fs::read_dir("/dev/disk/by-uuid") {
        Ok(d) => d,
        Err(_) => return m,
    };
    for entry in dir.flatten() {
        let uuid = entry.file_name();
        let uuid = uuid.to_string_lossy();
        if uuid.is_empty() {
            continue;
        }
        let link_path = Path::new("/dev/disk/by-uuid").join(uuid.as_ref());
        if let Ok(resolved) = std::fs::canonicalize(&link_path) {
            m.insert(resolved, uuid.to_string());
        }
    }
    m
}

// ---------------------------------------------------------------------------
// Rebuild mount cache from /proc/mounts
// ---------------------------------------------------------------------------

fn rebuild_cache() -> MountCache {
    let dev_to_uuid = build_dev_to_uuid_map();
    let mut fsid_by_mount = HashMap::with_capacity(64);

    let file = std::fs::File::open("/proc/mounts");
    if let Ok(file) = file {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let src = decode_fstab_escapes(parts[0]);
            let mp = decode_fstab_escapes(parts[1]);
            let mp = Path::new(&mp).components().collect::<PathBuf>();
            let mp = mp.to_string_lossy().to_string();
            if mp.is_empty() {
                continue;
            }
            if fsid_by_mount.contains_key(&mp) {
                continue;
            }
            let fsid = if src.starts_with("/dev/") {
                let resolved = std::fs::canonicalize(&src).unwrap_or_else(|_| PathBuf::from(&src));
                dev_to_uuid
                    .get(&resolved)
                    .cloned()
                    .unwrap_or_else(|| resolved.to_string_lossy().to_string())
            } else {
                src.clone()
            };
            fsid_by_mount.insert(mp.clone(), fsid);
        }
    }

    // Root always resolves (empty fsid when unknown), so the lookup below
    // never misses entirely.
    let fsid_of_root = fsid_by_mount.entry("/".to_string()).or_default().clone();

    // Longest mountpoint first so a nested mount wins over its parent.
    let mut table: MountTable = fsid_by_mount
        .into_iter()
        .filter_map(|(mp, fsid)| {
            if mp == "/" {
                None // handled by fsid_of_root
            } else {
                Some((format!("{mp}/"), fsid))
            }
        })
        .collect();
    table.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    MountCache {
        table,
        fsid_of_root,
        built_at: std::time::Instant::now(),
    }
}

/// Cached mount table. Cloning is a single `Arc` bump — the scan hot path
/// calls this once per file.
fn ensure_cache() -> Arc<MountCache> {
    let mut guard = CACHE.lock().unwrap();
    let needs_rebuild = guard
        .as_ref()
        .map(|c| c.built_at.elapsed() > TTL)
        .unwrap_or(true);
    if needs_rebuild {
        *guard = Some(Arc::new(rebuild_cache()));
    }
    Arc::clone(guard.as_ref().unwrap())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve the filesystem UUID for a given file path.
pub fn filesystem_id_for_path(path: &str) -> String {
    let path = Path::new(path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let path_str = path.to_string_lossy();

    let cache = ensure_cache();
    for (mp_slash, fsid) in &cache.table {
        if path_str.starts_with(mp_slash.as_str()) {
            return fsid.clone();
        }
    }
    cache.fsid_of_root.clone()
}

/// Read (inode, ctime) from an already-obtained stat, so the scan hot path
/// pays one stat per file instead of one per subsystem.
pub fn identity_from_metadata(meta: &std::fs::Metadata) -> (u64, i64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.ino(), meta.ctime())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let created = meta.creation_time();
        let unix_seconds = created.saturating_sub(116_444_736_000_000_000) / 10_000_000;
        (meta.file_index().unwrap_or_default(), unix_seconds as i64)
    }
}

/// Derive a stable UUID from (fsuuid, inode, ctime). Same algorithm as
/// Go's `uuidFromTriplet`: SHA-1(namespace + "fsuuid:ino:ctime"),
/// formatted as a version-5-ish UUID.
pub fn uuid_from_triplet(fsuuid: &str, ino: u64, ctime: i64) -> String {
    let s = format!("{fsuuid}:{ino}:{ctime}");
    let mut hasher = Sha1::new();
    hasher.update(UUID_NAMESPACE);
    hasher.update(s.as_bytes());
    let sum = hasher.finalize();
    let mut b = [0u8; 16];
    b.copy_from_slice(&sum[..16]);
    // Version 5 variant bits (same as Go).
    b[6] = (b[6] & 0x0f) | 0x50;
    b[8] = (b[8] & 0x3f) | 0x80;
    let x = crate::utils::hex::bytes_to_hex(&b);
    format!(
        "{}-{}-{}-{}-{}",
        &x[0..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..32]
    )
}

/// Same as a stat + [`uuid_from_triplet`] pipeline but reusing an
/// already-obtained stat (the walker stats every entry anyway).
pub fn generate_uuid_from_metadata(
    path: &str,
    meta: &std::fs::Metadata,
) -> (String, String, u64, i64) {
    let (ino, ctime) = identity_from_metadata(meta);
    let mut fsuuid = filesystem_id_for_path(path);
    if fsuuid.is_empty() {
        fsuuid = "/".to_string();
    }
    let id = uuid_from_triplet(&fsuuid, ino, ctime);
    (id, fsuuid, ino, ctime)
}

/// Secondary-index key for (fsuuid, inode, ctime) → uuid.
pub fn fid_key(fsuuid: &str, ino: u64, ctime: i64) -> String {
    let hash = xxhash_rust::xxh64::xxh64(fsuuid.as_bytes(), 0);
    format!("{FID_INDEX_PREFIX}{hash:016x}:{ino}:{ctime}")
}

/// Find an existing UUID by (fsuuid, inode, ctime) via the `media:fid:`
/// secondary index. Returns `None` if not found.
pub fn find_uuid_by_fid(
    db: &crate::media::kv::Db,
    fsuuid: &str,
    ino: u64,
    ctime: i64,
) -> Option<String> {
    let key = fid_key(fsuuid, ino, ctime);
    db.get(key.as_bytes())
        .ok()
        .flatten()
        .map(|v| String::from_utf8_lossy(&v).to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/media/uuid.rs"]
mod tests;
