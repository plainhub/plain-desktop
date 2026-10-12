use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn walk_files(
    root: &Path,
    mut excluded: impl FnMut(&Path) -> bool,
    mut cancelled: impl FnMut() -> bool,
    mut visit: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    if !root.is_absolute() {
        return Err(anyhow::anyhow!(
            "file walk root is not an absolute path: {}",
            root.display()
        ));
    }
    if cancelled() {
        return Err(cancelled_error(root));
    }
    let root_meta = fs::symlink_metadata(root)
        .with_context(|| format!("cannot read file walk root {}", root.display()))?;
    if root_meta.file_type().is_symlink() {
        return Err(anyhow::anyhow!(
            "file walk root is a symlink and is not followed: {}",
            root.display()
        ));
    }
    if !root_meta.is_file() && !root_meta.is_dir() {
        return Err(anyhow::anyhow!(
            "file walk root is neither a regular file nor a directory: {}",
            root.display()
        ));
    }
    if excluded(root) {
        return Ok(());
    }
    if root_meta.is_file() {
        if cancelled() {
            return Err(cancelled_error(root));
        }
        visit(root).with_context(|| format!("cannot visit {}", root.display()))?;
        return Ok(());
    }

    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        if cancelled() {
            return Err(cancelled_error(root));
        }
        let entries = fs::read_dir(&directory)
            .with_context(|| format!("cannot read directory {}", directory.display()))?;
        let mut subdirectories = Vec::new();
        for entry in entries {
            let entry = entry
                .with_context(|| format!("cannot read an entry of {}", directory.display()))?;
            if cancelled() {
                return Err(cancelled_error(root));
            }
            let path = entry.path();
            if excluded(&path) {
                continue;
            }
            let file_type = entry
                .file_type()
                .with_context(|| format!("cannot read the file type of {}", path.display()))?;
            if file_type.is_dir() {
                subdirectories.push(path);
            } else if file_type.is_file() {
                if cancelled() {
                    return Err(cancelled_error(root));
                }
                visit(&path).with_context(|| format!("cannot visit {}", path.display()))?;
            }
        }
        stack.extend(subdirectories.into_iter().rev());
    }
    Ok(())
}

fn cancelled_error(root: &Path) -> anyhow::Error {
    anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        format!("file walk cancelled: {}", root.display()),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/file_walk.rs"]
mod tests;
