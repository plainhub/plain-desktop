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
pub fn claim(tx: &Transaction<'_>, additions: BTreeMap<String, i64>) -> Result<()> {
    for (id, count) in additions {
        let refs: i64 = tx
            .query_row("SELECT ref_count FROM app_files WHERE id=?1", [&id], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("Unknown app file reference"))?;
        let used: i64 = tx.query_row("SELECT count(*) FROM chats c,json_tree(c.content) j WHERE j.type='text' AND ((json_extract(c.content,'$.type') IN ('FILES','IMAGES') AND j.key='uri' AND j.path LIKE '$.value.items[%]') OR (json_extract(c.content,'$.type')='TEXT' AND j.key='imageLocalPath' AND j.path LIKE '$.value.linkPreviews[%]')) AND substr(j.atom,1,4)='fid:' AND (CASE WHEN instr(substr(j.atom,5),'.')=0 THEN substr(j.atom,5) ELSE substr(j.atom,5,instr(substr(j.atom,5),'.')-1) END)=?1",[&id],|r|r.get(0))?;
        let reserved = refs
            .checked_sub(used)
            .filter(|n| *n >= 0)
            .ok_or_else(|| anyhow::anyhow!("App file reference accounting failed"))?;
        let extra = count.saturating_sub(reserved).max(0);
        let next = refs
            .checked_add(extra)
            .filter(|n| *n <= i64::from(i32::MAX))
            .ok_or_else(|| anyhow::anyhow!("App file reference count overflow"))?;
        if extra > 0 {
            tx.execute(
                "UPDATE app_files SET ref_count=?2,updated_at=?3 WHERE id=?1",
                params![id, next, crate::db::now_iso()],
            )?;
        }
    }
    Ok(())
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

pub fn release_unbound(
    db: &crate::db::Db,
    directory: &Path,
    imports: BTreeMap<String, i64>,
) -> Result<()> {
    let _files = db.app_files_lock()?;
    let mut staged = vec![];
    let result=db.with_conn(|c| -> Result<()> {
        let tx=c.unchecked_transaction()?;
        let mut releases=BTreeMap::new();
        for (id,count) in imports {
            let refs: Option<i64>=tx.query_row("SELECT ref_count FROM app_files WHERE id=?1",[&id],|r|r.get(0)).optional()?;
            let Some(refs)=refs else { continue };
            let used:i64=tx.query_row("SELECT count(*) FROM chats c,json_tree(c.content) j WHERE j.type='text' AND ((json_extract(c.content,'$.type') IN ('FILES','IMAGES') AND j.key='uri' AND j.path LIKE '$.value.items[%]') OR (json_extract(c.content,'$.type')='TEXT' AND j.key='imageLocalPath' AND j.path LIKE '$.value.linkPreviews[%]')) AND substr(j.atom,1,4)='fid:' AND (CASE WHEN instr(substr(j.atom,5),'.')=0 THEN substr(j.atom,5) ELSE substr(j.atom,5,instr(substr(j.atom,5),'.')-1) END)=?1",[&id],|r|r.get(0))?;
            let count=count.min(refs.saturating_sub(used)).max(0);
            if count>0 { releases.insert(id,count); }
        }
        release(&tx,directory,releases,&mut staged)?;
        tx.commit()?; Ok(())
    });
    finish(result, staged)
}
