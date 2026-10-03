use crate::{
    db::Db,
    library::{
        LibraryError, LibraryResult,
        image_embeddings::{self, EmbeddingInput},
    },
};
use std::collections::HashSet;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub id: String,
    pub path: String,
}
#[derive(serde::Deserialize)]
pub struct Snapshot {
    pub revision: String,
    pub total: usize,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub revision: String,
    pub items: Vec<Image>,
    pub next_cursor: String,
    pub done: bool,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Embedded {
    pub items: Vec<EmbeddingInput>,
    pub skipped_ids: Vec<String>,
}
#[derive(Clone, Copy, Default)]
pub struct Progress {
    pub total: usize,
    pub indexed: usize,
    pub skipped: usize,
}
pub trait Provider {
    fn snapshot(&mut self) -> LibraryResult<Snapshot>;
    fn page(&mut self, revision: &str, cursor: &str, limit: usize) -> LibraryResult<Page>;
    fn embed(&mut self, revision: &str, items: &[Image]) -> LibraryResult<Embedded>;
    fn resolve(&mut self, _revision: &str, _ids: &[String]) -> LibraryResult<Vec<Image>> {
        Err(invalid("image selection unavailable"))
    }
    fn verify(&mut self, revision: &str) -> LibraryResult<()>;
}
pub trait Control {
    fn check(&self) -> LibraryResult<()>;
    fn save(&self, db: &Db, items: &[EmbeddingInput]) -> LibraryResult<()>;
    fn remove(&self, db: &Db, ids: &[String]) -> LibraryResult<()>;
    fn progress(&self, progress: Progress);
}
fn invalid(message: &str) -> LibraryError {
    LibraryError::Other(message.into())
}
pub fn scan(
    db: &Db,
    provider: &mut impl Provider,
    control: &impl Control,
    force: bool,
) -> LibraryResult<Progress> {
    control.check()?;
    let snapshot = provider.snapshot()?;
    if snapshot.revision.is_empty() {
        return Err(invalid("missing image catalog revision"));
    }
    let existing = image_embeddings::ids(db)?
        .into_iter()
        .collect::<HashSet<_>>();
    let mut current = HashSet::new();
    let mut cursor = String::new();
    let mut cursors = HashSet::new();
    let mut progress = Progress {
        total: snapshot.total,
        ..Default::default()
    };
    control.progress(progress);
    loop {
        control.check()?;
        let page = provider.page(&snapshot.revision, &cursor, 128)?;
        if !page.done && page.items.is_empty() {
            return Err(invalid("empty image catalog page"));
        }
        if page.revision != snapshot.revision || page.items.len() > 128 {
            return Err(invalid("image catalog changed"));
        }
        for image in &page.items {
            if image.id.is_empty() || image.path.is_empty() || !current.insert(image.id.clone()) {
                return Err(invalid("invalid image catalog page"));
            }
        }
        if current.len() > snapshot.total {
            return Err(invalid("image catalog count mismatch"));
        }
        let missing = page
            .items
            .into_iter()
            .filter(|i| {
                if !force && existing.contains(&i.id) {
                    progress.indexed += 1;
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        for batch in missing.chunks(20) {
            embed_batch(
                db,
                provider,
                control,
                &snapshot.revision,
                batch,
                &mut progress,
            )?;
        }
        control.progress(progress);
        if page.done {
            break;
        }
        if page.next_cursor.is_empty() || !cursors.insert(page.next_cursor.clone()) {
            return Err(invalid("invalid image catalog cursor"));
        }
        cursor = page.next_cursor;
    }
    control.check()?;
    if current.len() != snapshot.total {
        return Err(invalid("incomplete image catalog"));
    }
    provider.verify(&snapshot.revision)?;
    control.check()?;
    control.remove(
        db,
        &existing.difference(&current).cloned().collect::<Vec<_>>(),
    )?;
    Ok(progress)
}
#[cfg(test)]
#[path = "../../tests/unit/library/image_indexing.rs"]
mod tests;

fn embed_batch(
    db: &Db,
    provider: &mut impl Provider,
    control: &impl Control,
    revision: &str,
    batch: &[Image],
    progress: &mut Progress,
) -> LibraryResult<()> {
    control.check()?;
    let embedded = provider.embed(revision, batch)?;
    control.check()?;
    let requested = batch
        .iter()
        .map(|i| (&i.id, &i.path))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen = HashSet::new();
    for item in &embedded.items {
        if requested
            .get(&item.id)
            .is_none_or(|path| **path != item.path)
            || !seen.insert(item.id.clone())
        {
            return Err(invalid("invalid image embedding reply"));
        }
    }
    for id in &embedded.skipped_ids {
        if !requested.contains_key(id) || !seen.insert(id.clone()) {
            return Err(invalid("invalid skipped image reply"));
        }
    }
    if seen.len() != batch.len() {
        return Err(invalid("incomplete image embedding reply"));
    }
    control.save(db, &embedded.items)?;
    progress.indexed += embedded.items.len();
    progress.skipped += embedded.skipped_ids.len();
    control.progress(*progress);
    Ok(())
}

pub fn selected(
    db: &Db,
    provider: &mut impl Provider,
    control: &impl Control,
    ids: &[String],
) -> LibraryResult<Progress> {
    if ids.is_empty() {
        return Ok(Progress::default());
    }
    if ids.len() > 4096 || ids.iter().any(|id| id.is_empty()) {
        return Err(invalid("invalid image index selection"));
    }
    control.check()?;
    let snapshot = provider.snapshot()?;
    if snapshot.revision.is_empty() {
        return Err(invalid("missing image catalog revision"));
    }
    let existing = image_embeddings::ids(db)?
        .into_iter()
        .collect::<HashSet<_>>();
    let requested = ids.iter().cloned().collect::<HashSet<_>>();
    let mut found = HashSet::new();
    let mut progress = Progress {
        total: snapshot.total,
        indexed: image_embeddings::count(db)?
            .try_into()
            .map_err(|_| invalid("invalid image embedding count"))?,
        skipped: 0,
    };
    let mut unique = requested.iter().cloned().collect::<Vec<_>>();
    unique.sort();
    for batch in unique.chunks(100) {
        control.check()?;
        let rows = provider.resolve(&snapshot.revision, batch)?;
        if rows.len() > batch.len() {
            return Err(invalid("invalid image selection reply"));
        }
        let batch_ids = batch.iter().collect::<HashSet<_>>();
        let mut missing = Vec::new();
        for row in rows {
            if !batch_ids.contains(&row.id) || row.path.is_empty() || !found.insert(row.id.clone())
            {
                return Err(invalid("invalid image selection reply"));
            }
            if !existing.contains(&row.id) {
                missing.push(row);
            }
        }
        for images in missing.chunks(20) {
            embed_batch(
                db,
                provider,
                control,
                &snapshot.revision,
                images,
                &mut progress,
            )?;
        }
    }
    control.check()?;
    provider.verify(&snapshot.revision)?;
    control.check()?;
    control.remove(
        db,
        &requested.difference(&found).cloned().collect::<Vec<_>>(),
    )?;
    progress.indexed = image_embeddings::count(db)?
        .try_into()
        .map_err(|_| invalid("invalid image embedding count"))?;
    control.progress(progress);
    Ok(progress)
}
