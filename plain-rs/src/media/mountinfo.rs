//! Parsing of `/proc/self/mountinfo` and mountpoint resolution.
//! Port of `internal/fs/mountinfo.go`.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MountInfoEntry {
    pub mount_point: String,
}

fn decode_mount_escapes(s: &str) -> String {
    // mountinfo encodes space/tab/newline/backslash as octal escapes.
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

pub fn read_mountinfo() -> anyhow::Result<Vec<MountInfoEntry>> {
    let raw = fs::read_to_string("/proc/self/mountinfo")?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Split on " - " separator; superblock fields come after.
        let (left, _right) = match line.split_once(" - ") {
            Some(v) => v,
            None => continue,
        };
        let mut parts = left.split_whitespace();
        // Fields: mount id, parent id, dev, source, mount point
        parts.next(); // mount id
        parts.next(); // parent id
        parts.next(); // dev
        parts.next(); // source
        let mp_raw = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let mp = decode_mount_escapes(mp_raw);
        if mp.is_empty() {
            continue;
        }
        out.push(MountInfoEntry { mount_point: mp });
    }
    if out.is_empty() {
        anyhow::bail!("no mountinfo entries");
    }
    Ok(out)
}

fn find_best_mount_point(entries: &[MountInfoEntry], abs_path: &str) -> Option<String> {
    let mut best: Option<String> = None;
    for e in entries {
        let mp = e.mount_point.trim();
        if mp.is_empty() {
            continue;
        }
        let matched = if abs_path == mp {
            true
        } else if mp == "/" {
            Path::new(abs_path).is_absolute()
        } else {
            abs_path.starts_with(mp) && abs_path.as_bytes().get(mp.len()) == Some(&b'/')
        };
        if matched {
            match &best {
                None => best = Some(mp.to_string()),
                Some(b) if mp.len() > b.len() => best = Some(mp.to_string()),
                _ => {}
            }
        }
    }
    best
}

/// Resolve `abs_path` to the longest mountpoint prefix that contains it.
pub fn resolve_mount_point<P: AsRef<Path>>(abs_path: P) -> anyhow::Result<String> {
    let abs = abs_path.as_ref().clean_path();
    if abs.as_os_str().is_empty() || abs == PathBuf::from(".") {
        anyhow::bail!("invalid path");
    }
    let abs_str = match abs.to_str() {
        Some(s) => s.to_string(),
        None => anyhow::bail!("invalid utf-8 path"),
    };

    let entries = read_mountinfo()?;
    if let Some(best) = find_best_mount_point(&entries, &abs_str) {
        return Ok(best);
    }

    // Fallback: try resolving symlinks and look up the real path.
    if let Ok(real) = std::fs::canonicalize(&abs) {
        let real_str = real.to_string_lossy().to_string();
        if real_str != abs_str {
            if let Some(best) = find_best_mount_point(&entries, &real_str) {
                return Ok(best);
            }
        }
    }
    anyhow::bail!("unable to resolve mountpoint")
}

/// `filepath.Clean` for a `Path`.
pub trait CleanPath {
    fn clean_path(&self) -> PathBuf;
}
impl CleanPath for Path {
    fn clean_path(&self) -> PathBuf {
        path_clean(self)
    }
}
impl CleanPath for PathBuf {
    fn clean_path(&self) -> PathBuf {
        path_clean(self.as_path())
    }
}

fn path_clean(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let mut out = PathBuf::new();
    let leading_slash = s.starts_with('/');
    for comp in s.split('/').filter(|p| !p.is_empty()) {
        match comp {
            "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    if leading_slash {
        out = PathBuf::from("/").join(out);
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/media/mountinfo.rs"]
mod tests;
