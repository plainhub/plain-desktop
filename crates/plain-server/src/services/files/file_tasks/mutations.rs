use crate::services::files::file_tasks::*;
use plain_rs::files::filesystem::deletion::Outcome;
use plain_rs::files::filesystem::deletion::Plan;
use plain_rs::files::tasks::FileTaskOp;
use plain_rs::files::tasks::FileTaskStatus;
use plain_rs::files::tasks::FileTaskType;
use plain_rs::files::tasks::Hooks;
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
        let client_id = self
            .hooks
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("file task identity unavailable"))?;
        let service = self.service()?;
        let task = service.rename(&client_id, path, destination.clone())?;
        loop {
            let current = match service.get(&task.id)? {
                Some(current) => current,
                None => self
                    .list(client_id.clone())
                    .await?
                    .into_iter()
                    .find(|row| row.id == task.id)
                    .ok_or_else(|| anyhow!("rename task unavailable"))?,
            };
            match current.status {
                FileTaskStatus::Done => break,
                FileTaskStatus::Error if current.completed_ops.is_empty() => return Ok(None),
                FileTaskStatus::Error => bail!(
                    "rename postprocessing requires recovery for task {}: {}",
                    current.id,
                    current.error
                ),
                _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        Ok(Some(destination))
    }
    pub async fn delete(&self, path: String) -> Result<Outcome> {
        let _guard = self.deletion_lock.lock().await;
        plain_rs::files::filesystem::deletion::validate(std::path::Path::new(&path))?;
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
        let root = crate::services::files::file_task_media::normalized(std::path::Path::new(&path))
            .await?;
        let checked = root.clone();
        let plan =
            tokio::task::spawn_blocking(move || Plan::inspect(std::path::Path::new(&checked)))
                .await??;
        let snapshot = crate::services::files::file_task_media::deletion_snapshot(
            &self.hooks.host,
            root.clone(),
            &plan.files,
        )
        .await?;
        self.hooks
            .call("fileTaskAuthorize", json!({"paths":[root]}))
            .await?;
        let intent = deletion_recovery::Intent {
            id: plain_rs::short_uuid::short_uuid(),
            path,
            root,
            planned: plan.paths(),
            snapshot,
            outcome: None,
        };
        intent.save(&self.db)?;
        let outcome = tokio::task::spawn_blocking(move || plan.execute()).await?;
        let intent = deletion_recovery::Intent {
            outcome: Some(outcome.clone()),
            ..intent
        };
        intent.save(&self.db)?;
        self.finish_deletion(&intent, &outcome).await?;
        Ok(outcome)
    }
    pub(crate) async fn finish_deletion(
        &self,
        intent: &deletion_recovery::Intent,
        outcome: &Outcome,
    ) -> Result<()> {
        let root = intent.root.clone();
        let path = intent.path.clone();
        let snapshot = intent.snapshot.clone();
        for path in &outcome.paths {
            deletion_recovery::require_absent(path)?;
        }
        if !outcome.paths.is_empty() {
            let items =
                crate::services::files::file_task_media::deleted_items(&snapshot, &outcome.paths)?;
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
                    use plain_rs::files::deletes as media_deletes;
                    use plain_rs::media::audio::commands as audio_commands;
                    use plain_rs::media::audio::playback as audio_playback;
                    let before = audio_playback::snapshot(db)?;
                    let stop = removed_paths.contains(&before.path)
                        || roots.iter().any(|root| {
                            before.path == *root
                                || before
                                    .path
                                    .starts_with(&format!("{}/", root.trim_end_matches('/')))
                        });
                    let cleanup = |db: &Db| {
                        for path in &removed_paths {
                            deletion_recovery::require_absent(path)
                                .map_err(|e| plain_rs::error::DomainError::Other(e.to_string()))?;
                        }
                        media_deletes::cleanup(db, &items, &removed_paths, &roots)
                    };
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
            let _ = self
                .events
                .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
            for paths in outcome.paths.chunks(128) {
                self.hooks
                    .call("fileTaskScan", json!({"paths":paths}))
                    .await?;
            }
        }
        intent.remove(&self.db)?;
        Ok(())
    }
}
