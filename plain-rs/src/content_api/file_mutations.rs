use super::*;
use crate::filesystem::{
    deletion::{Outcome, Plan},
    tasks::{CompletedOp, FileTaskOp, FileTaskType, Hooks},
};
impl FileTasks {
    pub async fn rename(&self, path: String, name: String) -> Result<Option<String>> {
        if name.is_empty()
            || matches!(name.as_str(), "." | "..")
            || name.contains('/')
            || name.contains('\\')
        {
            return Ok(None);
        }
        let source = PathBuf::from(&path);
        if !source.is_absolute() || source.file_name().is_none() {
            bail!("absolute file path required");
        }
        let destination = source
            .parent()
            .ok_or_else(|| anyhow!("missing file parent"))?
            .join(&name);
        let destination = destination
            .to_str()
            .ok_or_else(|| anyhow!("invalid filename encoding"))?
            .to_owned();
        let op = FileTaskOp {
            src: path.clone(),
            dst: destination.clone(),
            overwrite: false,
        };
        self.hooks
            .authorize(FileTaskType::Move, std::slice::from_ref(&op))
            .await?;
        if tokio::fs::symlink_metadata(&destination).await.is_ok() {
            return Ok(None);
        }
        let snapshot = self.hooks.prepare(FileTaskType::Move, &op).await?;
        self.hooks
            .authorize(FileTaskType::Move, std::slice::from_ref(&op))
            .await?;
        if crate::filesystem::rename(&source, std::path::Path::new(&destination))
            .await
            .is_err()
        {
            return Ok(None);
        }
        self.hooks
            .completed_with_snapshot(
                FileTaskType::Move,
                &CompletedOp {
                    recovery: None,
                    src: path,
                    dst: destination.clone(),
                },
                &snapshot,
            )
            .await?;
        let _ = self.events.send(WsEvent::broadcast(47, "{}".into()));
        Ok(Some(destination))
    }
    pub async fn delete(&self, path: String) -> Result<Outcome> {
        crate::filesystem::deletion::validate(std::path::Path::new(&path))?;
        self.hooks
            .call("fileTaskAuthorize", json!({"paths":[path]}))
            .await?;
        match tokio::fs::symlink_metadata(&path).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Outcome {
                    removed: false,
                    paths: Vec::new(),
                    failures: Vec::new(),
                });
            }
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let root = super::super::file_task_media::normalized(std::path::Path::new(&path)).await?;
        let checked = root.clone();
        let plan =
            tokio::task::spawn_blocking(move || Plan::inspect(std::path::Path::new(&checked)))
                .await??;
        let snapshot = super::super::file_task_media::deletion_snapshot(
            &self.hooks.host,
            root.clone(),
            &plan.files,
        )
        .await?;
        self.hooks
            .call("fileTaskAuthorize", json!({"paths":[root]}))
            .await?;
        let outcome = tokio::task::spawn_blocking(move || plan.execute()).await?;
        if !outcome.paths.is_empty() {
            let items = super::super::file_task_media::deleted_items(&snapshot, &outcome.paths)?;
            let mut removed_paths = outcome.paths.clone();
            for removed in &outcome.paths {
                let relative = std::path::Path::new(removed).strip_prefix(&root)?;
                let alias = if relative.as_os_str().is_empty() {
                    path.clone()
                } else {
                    std::path::Path::new(&path)
                        .join(relative)
                        .to_string_lossy()
                        .into_owned()
                };
                if !removed_paths.contains(&alias) {
                    removed_paths.push(alias);
                }
            }
            let roots = if outcome.removed {
                vec![root.clone(), path.clone()]
            } else {
                Vec::new()
            };
            let prefs = self.hooks.prefs.clone();
            let index = self.hooks.index.clone();
            self.hooks
                .audio
                .run(move |db, engine| {
                    use crate::library::{audio_commands, audio_playback, media_deletes};
                    let before = audio_playback::snapshot(db)?;
                    let stop = removed_paths.contains(&before.path)
                        || roots.iter().any(|root| {
                            before.path == *root
                                || before
                                    .path
                                    .starts_with(&format!("{}/", root.trim_end_matches('/')))
                        });
                    let cleanup =
                        |db: &Db| media_deletes::cleanup(db, &items, &removed_paths, &roots);
                    index.update_cache(cleanup)?;
                    if stop && !before.path.is_empty() {
                        audio_commands::command(
                            db,
                            &prefs,
                            engine,
                            audio_commands::Action::Clear,
                            0,
                            1.0,
                        )?;
                    }
                    Ok(())
                })
                .await
                .map_err(|e| anyhow!(e.message))?;
            let _ = self.events.send(WsEvent::broadcast(47, "{}".into()));
            for paths in outcome.paths.chunks(128) {
                self.hooks
                    .call("fileTaskScan", json!({"paths":paths}))
                    .await?;
            }
        }
        Ok(outcome)
    }
}
