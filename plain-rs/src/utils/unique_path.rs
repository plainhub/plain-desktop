//! Filesystem naming helper shared by plain-nas and plain-desktop: pick a
//! non-conflicting sibling path for a target that already exists, using the
//! `name_1.ext` convention (unified 2026-09-04 — plain-nas previously used
//! `name (1).ext`).

use std::path::{Path, PathBuf};

/// If `target` does not exist, return it unchanged. Otherwise return the
/// first non-existing sibling of the form `stem_N.ext` (N from 1 to 9999).
/// The split keeps dot-directories intact (`.gitignore` → `.gitignore_1`).
/// If all 9999 candidates are taken, `target` is returned as-is.
pub fn unique_sibling(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let base = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let (stem, ext) = match base.rfind('.') {
        // i > 0 keeps dotfiles (".gitignore") whole.
        Some(i) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    for n in 1..10000 {
        let candidate = parent.join(format!("{stem}_{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    target.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "plain-rs-uniq-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn non_existing_target_unchanged() {
        let dir = scratch("free");
        let t = dir.join("a.txt");
        assert_eq!(unique_sibling(&t), t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_target_gets_index_suffix() {
        let dir = scratch("basic");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(unique_sibling(&dir.join("a.txt")), dir.join("a_1.txt"));
        std::fs::write(dir.join("a_1.txt"), b"x").unwrap();
        assert_eq!(unique_sibling(&dir.join("a.txt")), dir.join("a_2.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extensionless_and_dotfile() {
        let dir = scratch("edge");
        std::fs::write(dir.join("README"), b"x").unwrap();
        assert_eq!(unique_sibling(&dir.join("README")), dir.join("README_1"));
        std::fs::write(dir.join(".gitignore"), b"x").unwrap();
        assert_eq!(
            unique_sibling(&dir.join(".gitignore")),
            dir.join(".gitignore_1")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
