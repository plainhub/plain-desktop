//! Recursive directory walker — a minimal replacement for the parts of the
//! `walkdir` crate we actually use.
//!
//! Why not `walkdir`?
//! ------------------
//! `walkdir` is a small but non-trivial crate (it also pulls in `same-file`)
//! and we only ever use a tiny subset of its API:
//!
//!   * `WalkDir::new(path).into_iter()` — produce `Result<DirEntry, ...>` for
//!     every entry, depth-first, never follow symlinks.
//!
//! That's a 30-line BFS / DFS in the standard library. We don't need
//! sorting by inode, depth limits, content filtering, or any of the
//! other knobs `walkdir` offers. Less code, less supply-chain surface.
//!
//! We deliberately expose the same shape (`IntoIterator<Item = DirEntry>`)
//! so call-sites change minimally: just replace
//! `walkdir::WalkDir::new(p).into_iter().filter_map(|e| e.ok())` with
//! `crate::media::walk::Walk::new(p).into_iter().filter_map(|e| e.ok())`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One entry produced by the walker. Mirrors `walkdir::DirEntry` for the
/// fields we actually use: `path()`, `file_type()`, `metadata()`,
/// `file_name()`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    path: PathBuf,
    file_type: fs::FileType,
    /// Cached for `metadata()`. We use `symlink_metadata` so symlinks are
    /// never followed (matches `walkdir::WalkDir::follow_links(false)`).
    metadata: fs::Metadata,
}

impl DirEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn file_type(&self) -> fs::FileType {
        self.file_type
    }
    pub fn metadata(&self) -> io::Result<fs::Metadata> {
        Ok(self.metadata.clone())
    }
    pub fn file_name(&self) -> std::ffi::OsString {
        // Equivalent to walkdir's `file_name`: the last path component.
        self.path
            .file_name()
            .map(|s| s.to_os_string())
            .unwrap_or_default()
    }
}

/// Walker builder. We don't support any options — every call site passed
/// `.follow_links(false)` and nothing else.
pub struct Walk {
    root: PathBuf,
}

impl Walk {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self { root: root.into() }
    }
}

impl IntoIterator for Walk {
    type Item = io::Result<DirEntry>;
    type IntoIter = WalkIter;
    fn into_iter(self) -> Self::IntoIter {
        WalkIter {
            stack: vec![self.root],
        }
    }
}

pub struct WalkIter {
    stack: Vec<PathBuf>,
}

impl Iterator for WalkIter {
    type Item = io::Result<DirEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        let path = self.stack.pop()?;
        // `symlink_metadata` so we never follow symlinks (matches
        // `walkdir`'s default + `.follow_links(false)`).
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => return Some(Err(e)),
        };
        let ft = meta.file_type();
        if ft.is_dir() {
            // Push children onto the stack in reverse order so the
            // first child is popped first (depth-first, matches walkdir's
            // ordering for the kinds of code that iterates and consumes
            // top-to-bottom in directory listings).
            match fs::read_dir(&path) {
                Ok(rd) => {
                    let mut children: Vec<PathBuf> =
                        rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
                    children.reverse();
                    self.stack.extend(children);
                }
                Err(e) => return Some(Err(e)),
            }
        }
        Some(Ok(DirEntry {
            path,
            file_type: ft,
            metadata: meta,
        }))
    }
}
