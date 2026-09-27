//! Search DSL (parser in plain-rs) + walkdir-based file search.
//!
//! 1:1 port of
//! `internal/graph/helpers/files_helper.go::SearchFiles`.
//!
//! The Go side has a full offline mmap-based inverted index
//! (`fs_index_search.go`) that we don't have. For the MVP we use
//! `SearchFiles` (walkdir, in-memory) so the GraphQL `files(query: ...)` query
//! still works against the live filesystem. The DSL parser is the same as the
//! Go side, so existing frontend query strings will keep working.
//!
//! ## Query syntax (Go `Parse` semantics, preserved)
//!
//! Tokens are split on whitespace, with single/double quotes preserving
//! internal spaces and `\` escaping the next char. Each token is parsed as
//! `field:op_value`:
//!   - `foo:bar`          → { Name: "foo", Op: "=", Value: "bar" }
//!   - `foo:>10`          → { Name: "foo", Op: ">", Value: "10" }
//!   - `foo:<=100`        → { Name: "foo", Op: "<=", Value: "100" }
//!   - `foo`              → { Name: "text", Op: "", Value: "foo" }
//!   - `is:dir`           → { Name: "dir", Op: "", Value: "true" }
//!   - `NOT foo:bar`      → inverts the next field's op (= ↔ !=, > ↔ <=, ...)
//!
//! Supported op map: `=`, `!=`, `>`, `>=`, `<`, `<=`, plus the notational `in`
//! / `nin` keys (mirrored for invert-completeness).

use anyhow::Result;
use std::path::Path;

// The DSL parser lives in plain-rs (`utils::search_dsl`) — shared with
// plain-desktop so both ends resolve `text:`/`ids:` fields identically.
pub use crate::utils::search_dsl::{FilterField, parse};

// ---------------------------------------------------------------------------
// Walkdir search (mirror files_helper.go::SearchFiles)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchedFile {
    pub path: String,
    pub is_dir: bool,
}

/// Walk `root` and return files whose **base name** contains `text` (case
/// insensitive). When `text` is empty, returns every entry. Hidden files
/// (dotfiles) are skipped unless `show_hidden` is set.
pub fn search_files(text: &str, root: &Path, show_hidden: bool) -> Vec<SearchedFile> {
    let lower = text.to_lowercase();
    let mut out: Vec<SearchedFile> = Vec::new();
    let walker = crate::media::walk::Walk::new(root);
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            if entry.file_type().is_dir() {
                continue;
            }
            continue;
        }
        let matched = text.is_empty() || name.to_lowercase().contains(&lower);
        if !matched {
            continue;
        }
        out.push(SearchedFile {
            path: entry.path().to_string_lossy().to_string(),
            is_dir: entry.file_type().is_dir(),
        });
    }
    out
}

/// Combined API mirroring `files_query_helper.go::SearchIndexFiles`. With
/// mmap index not available, we fall back to `search_files` for any
/// non-text/non-size query, and return empty for text queries against an
/// empty/unindexed corpus (the Go side does the same).
pub fn search_index_files(
    text: &str,
    base: &str,
    offset: usize,
    limit: usize,
    show_hidden: bool,
    size_op: &str,
    _size_bytes: u64,
) -> Result<Vec<SearchedFile>> {
    let parent = normalize_slash_dir(base);
    if !text.trim().is_empty() || !size_op.is_empty() {
        // The Go side would ask the mmap index. We don't have one — return
        // empty rather than do a full filesystem walk. The frontend will
        // fall back to a directory listing via `files` (no query).
        return Ok(Vec::new());
    }
    let items = search_files(text, &parent, show_hidden);
    let offset = offset.min(items.len());
    let end = if limit == 0 || limit > items.len() - offset {
        items.len()
    } else {
        offset + limit
    };
    Ok(items[offset..end].to_vec())
}

fn normalize_slash_dir(p: &str) -> std::path::PathBuf {
    let p = p.trim_start_matches('/');
    if p.is_empty() {
        std::path::PathBuf::from("/")
    } else {
        std::path::PathBuf::from(p)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../../tests/unit/media/search.rs"]
mod tests;
