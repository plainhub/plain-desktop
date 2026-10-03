use super::{file_task_walk::FileWalker, host::Host};
use crate::library::media_moves::Binding;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    media_type: i32,
    media_id: String,
    path: String,
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    root: String,
    items: Vec<Identity>,
}
async fn lookup(host: &Host, paths: &[String]) -> Result<Vec<Identity>> {
    let result = host
        .call("fileTaskMediaSnapshot", json!({"paths": paths}))
        .await
        .map_err(anyhow::Error::msg)?;
    let rows: Vec<Identity> = serde_json::from_value(result)?;
    let mut seen = HashSet::new();
    for row in &rows {
        if !matches!(row.media_type, 1 | 2 | 3 | 24)
            || row.media_id.is_empty()
            || !paths.contains(&row.path)
            || !seen.insert(&row.path)
        {
            bail!("invalid native media identity receipt");
        }
    }
    Ok(rows)
}
async fn normalized(path: &Path) -> Result<String> {
    let parent = tokio::fs::canonicalize(
        path.parent()
            .ok_or_else(|| anyhow!("missing file parent"))?,
    )
    .await?;
    Ok(parent
        .join(
            path.file_name()
                .ok_or_else(|| anyhow!("missing file name"))?,
        )
        .to_str()
        .ok_or_else(|| anyhow!("invalid path encoding"))?
        .to_owned())
}
pub(super) async fn prepare(host: &Host, source: &str) -> Result<Value> {
    let root = normalized(Path::new(source)).await?;
    let mut walker = FileWalker::new(Path::new(&root));
    let mut batch = Vec::new();
    let mut items = Vec::new();
    while let Some(path) = walker.next().await? {
        batch.push(
            path.to_str()
                .ok_or_else(|| anyhow!("invalid source encoding"))?
                .to_owned(),
        );
        if batch.len() == 128 {
            items.extend(lookup(host, &batch).await?);
            batch.clear();
        }
    }
    if !batch.is_empty() {
        items.extend(lookup(host, &batch).await?);
    }
    Ok(serde_json::to_value(Snapshot { root, items })?)
}
pub(super) struct Migration {
    pub bindings: Vec<Binding>,
    pub source_root: String,
    pub destination_root: String,
}
pub(super) async fn resolve(host: &Host, destination: &str, snapshot: &Value) -> Result<Migration> {
    let snapshot: Snapshot = serde_json::from_value(snapshot.clone())?;
    let destination = normalized(Path::new(destination)).await?;
    let mut bindings = Vec::new();
    for batch in snapshot.items.chunks(128) {
        let paths = batch
            .iter()
            .map(|row| -> Result<String> {
                let relative = Path::new(&row.path).strip_prefix(&snapshot.root)?;
                let path = if relative.as_os_str().is_empty() {
                    Path::new(&destination).to_owned()
                } else {
                    Path::new(&destination).join(relative)
                };
                Ok(path
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid destination encoding"))?
                    .to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        let rows = lookup(host, &paths).await?;
        for (source, path) in batch.iter().zip(paths) {
            let target = rows
                .iter()
                .find(|row| row.path == path && row.media_type == source.media_type)
                .ok_or_else(|| anyhow!("moved media identity missing: {path}"))?;
            bindings.push(Binding {
                media_type: source.media_type,
                source_id: source.media_id.clone(),
                destination_id: target.media_id.clone(),
                source_path: source.path.clone(),
                destination_path: path,
            });
        }
    }
    Ok(Migration {
        bindings,
        source_root: snapshot.root,
        destination_root: destination,
    })
}
