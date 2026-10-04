use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub fn collect(content: &Value) -> BTreeMap<String, i64> {
    let mut refs = BTreeMap::new();
    let uris: Vec<&str> = match content["type"].as_str() {
        Some("FILES" | "IMAGES") => content["value"]["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["uri"].as_str())
            .collect(),
        Some("TEXT") => content["value"]["linkPreviews"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["imageLocalPath"].as_str())
            .collect(),
        _ => Vec::new(),
    };
    for uri in uris {
        if let Some(suffix) = uri.strip_prefix("fid:") {
            let id = suffix.split('.').next().unwrap_or_default();
            if !id.is_empty() {
                *refs.entry(id.into()).or_default() += 1;
            }
        }
    }
    refs
}
pub fn release(
    tx: &Transaction<'_>,
    directory: &Path,
    releases: BTreeMap<String, i64>,
    staged: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    for (id, count) in releases {
        let file: Option<(String, i64)> = tx
            .query_row(
                "SELECT real_path,ref_count FROM app_files WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((relative, refs)) = file else {
            continue;
        };
        if refs > count {
            tx.execute(
                "UPDATE app_files SET ref_count=ref_count-?1,updated_at=?2 WHERE id=?3",
                params![count, crate::db::now_iso(), id],
            )?;
        } else {
            let path = super::owned_path(directory, &relative)?;
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if !metadata.is_file() {
                        bail!("attachment is not a regular file");
                    }
                    let quarantine = path.with_file_name(format!(
                        ".release-{}",
                        crate::utils::short_uuid::short_uuid()
                    ));
                    fs::rename(&path, &quarantine)?;
                    staged.push((path, quarantine));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            tx.execute("DELETE FROM app_files WHERE id=?1", [&id])?;
        }
    }
    Ok(())
}
pub fn finish<T>(result: Result<T>, staged: Vec<(PathBuf, PathBuf)>) -> Result<T> {
    match result {
        Ok(value) => {
            let mut failures = Vec::new();
            for (_, quarantine) in staged {
                if let Err(error) = fs::remove_file(&quarantine) {
                    failures.push(format!("{}: {error}", quarantine.display()));
                }
            }
            if !failures.is_empty() {
                bail!("attachment cleanup failed: {}", failures.join("; "));
            }
            Ok(value)
        }
        Err(error) => {
            let mut failures = Vec::new();
            for (path, quarantine) in staged.into_iter().rev() {
                if let Err(restore) = fs::rename(&quarantine, &path) {
                    failures.push(format!("{}: {restore}", path.display()));
                }
            }
            if !failures.is_empty() {
                bail!(
                    "{error}; attachment restore failed: {}",
                    failures.join("; ")
                );
            }
            Err(error)
        }
    }
}
