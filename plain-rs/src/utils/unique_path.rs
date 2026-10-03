//! Filesystem naming helper shared by plain-nas and plain-desktop: pick a
//! non-conflicting sibling path for a target that already exists, using the
//! `name_1.ext` convention (unified 2026-09-04 — plain-nas previously used
//! `name (1).ext`).

use std::path::{Path, PathBuf};

/// If `target` does not exist, return it unchanged. Otherwise return the
/// first non-existing sibling of the form `stem_N.ext`.
/// The split keeps dot-directories intact (`.gitignore` → `.gitignore_1`).
/// Directory lookup failures are propagated.
pub fn unique_sibling(target: &Path) -> std::io::Result<PathBuf> {
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let base = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    let mut n = 0_u64;
    loop {
        let candidate = if n == 0 {
            target.to_path_buf()
        } else {
            parent.join(format!("{stem}_{n}{ext}"))
        };
        match std::fs::symlink_metadata(&candidate) {
            Ok(_) => {
                n = n
                    .checked_add(1)
                    .ok_or_else(|| std::io::Error::other("unique filename exhausted"))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Err(error) => return Err(error),
        }
    }
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
        assert_eq!(unique_sibling(&t).unwrap(), t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_target_gets_index_suffix() {
        let dir = scratch("basic");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(
            unique_sibling(&dir.join("a.txt")).unwrap(),
            dir.join("a_1.txt")
        );
        std::fs::write(dir.join("a_1.txt"), b"x").unwrap();
        assert_eq!(
            unique_sibling(&dir.join("a.txt")).unwrap(),
            dir.join("a_2.txt")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extensionless_and_dotfile() {
        let dir = scratch("edge");
        std::fs::write(dir.join("README"), b"x").unwrap();
        assert_eq!(
            unique_sibling(&dir.join("README")).unwrap(),
            dir.join("README_1")
        );
        std::fs::write(dir.join(".gitignore"), b"x").unwrap();
        assert_eq!(
            unique_sibling(&dir.join(".gitignore")).unwrap(),
            dir.join(".gitignore_1")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
