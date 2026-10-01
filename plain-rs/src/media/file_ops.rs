use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::media::{fsx, kv::Db, scan, trash};

pub async fn create_dir(path: &str) -> Result<fsx::FileEntry> {
    let path = Path::new(path);
    fsx::ensure_dir(path).await?;
    Ok(fsx::stat(path).await?)
}

pub async fn write_text_file(path: &str, content: &str, overwrite: bool) -> Result<fsx::FileEntry> {
    if content.len() > 2 * 1024 * 1024 {
        bail!("content too large");
    }
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        let parent_meta = tokio::fs::metadata(parent)
            .await
            .map_err(|error| anyhow::anyhow!("parent: {error}"))?;
        if !parent_meta.is_dir() {
            bail!("parent is not a directory");
        }
    }
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_dir() => bail!("path is a directory"),
        Ok(_) if !overwrite => bail!("target exists"),
        _ => {}
    }
    tokio::fs::write(path, content.as_bytes())
        .await
        .map_err(|error| anyhow::anyhow!("write: {error}"))?;
    Ok(fsx::stat(path).await?)
}

pub async fn rename_file(path: &str, name: &str) -> Result<()> {
    let path = PathBuf::from(path);
    let parent = path.parent().ok_or_else(|| anyhow::anyhow!("bad path"))?;
    fsx::rename(&path, &parent.join(name)).await?;
    Ok(())
}

pub async fn copy_file(src: &str, dst: &str, overwrite: bool) -> Result<()> {
    fsx::copy_path(Path::new(src), Path::new(dst), overwrite).await?;
    Ok(())
}

pub async fn move_file(src: &str, dst: &str, overwrite: bool) -> Result<()> {
    fsx::move_path(Path::new(src), Path::new(dst), overwrite).await?;
    Ok(())
}

pub async fn delete_files(db: &Db, paths: &[String]) -> Result<i32> {
    let mut affected = 0_i32;
    for raw in paths {
        if raw.trim() == "/" {
            bail!("refusing to delete root");
        }
        let _ = scan::delete_by_path(db, raw);
        let _ = scan::delete_by_path_prefix(db, raw);
        let path = Path::new(raw);
        let existed = tokio::fs::symlink_metadata(path).await.is_ok();
        match fsx::remove(path).await {
            Ok(()) if existed => affected += 1,
            Ok(()) => {}
            Err(error) => log::debug!("deleteFiles {raw}: {error}"),
        }
    }
    Ok(affected)
}

pub async fn trash_files(db: &Db, paths: Vec<String>) -> Result<i32> {
    let original = paths.clone();
    let trashed = trash::trash_paths(paths).await?;
    for path in &original {
        let _ = scan::delete_by_path(db, path);
        let _ = scan::delete_by_path_prefix(db, path);
    }
    Ok(i32::try_from(trashed.len()).unwrap_or(i32::MAX))
}

pub async fn restore_files(paths: Vec<String>) -> Result<i32> {
    let restored = trash::restore_paths(paths).await?;
    Ok(i32::try_from(restored.len()).unwrap_or(i32::MAX))
}

pub async fn delete_trash_item(path: &str) -> Result<()> {
    trash::delete_trash_by_path(path).await?;
    Ok(())
}
