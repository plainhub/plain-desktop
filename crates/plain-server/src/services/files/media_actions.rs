use crate::host::bridge::Host;
use crate::services::audio::host::Audio;
use crate::services::images::index::ImageIndex;
use async_graphql::Result;
use plain_rs::db::Db;
use plain_rs::enums::DataType;
use plain_rs::media::actions as media_actions;
use plain_rs::media::actions::Action;
use plain_rs::media::actions::Outcome;
use plain_rs::media::audio::commands as audio_commands;
use plain_rs::media::audio::playback as audio_playback;
use plain_rs::prefs::Prefs;
use serde_json::json;
use std::sync::Arc;
pub struct MediaActions {
    db: Arc<Db>,
    host: Arc<Host>,
    audio: Arc<Audio>,
    index: Arc<ImageIndex>,
    prefs: Arc<Prefs>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
}
pub struct Completed {
    pub affected_count: i32,
    pub failed_ids: Vec<String>,
}
impl MediaActions {
    pub fn new(
        db: Arc<Db>,
        host: Arc<Host>,
        audio: Arc<Audio>,
        index: Arc<ImageIndex>,
        prefs: Arc<Prefs>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    ) -> Self {
        Self {
            db,
            host,
            audio,
            index,
            prefs,
            events,
        }
    }
    pub async fn run_query(
        &self,
        kind: DataType,
        action: Action,
        query: &str,
        dest_dir: &str,
    ) -> Result<i32> {
        let from_trash = action == Action::Restore
            || (action == Action::Delete && {
                let facts = self
                    .host
                    .call("systemAppFacts", json!({}))
                    .await
                    .map_err(async_graphql::Error::new)?;
                crate::graphql::types::facts::strings(&facts, "capabilities")
                    .iter()
                    .any(|kind| kind == "MEDIA_TRASH")
            });
        let wire_kind = match kind {
            DataType::Audio => "AUDIO",
            DataType::Video => "VIDEO",
            DataType::Image => "IMAGE",
            DataType::Doc => "DOC",
            _ => return Err("unsupported media kind".into()),
        };
        let ids: Vec<String> = serde_json::from_value(
            self.host
                .call(
                    "systemMediaIdsFacts",
                    json!({"dataType":wire_kind,"query":query,"fromTrash":from_trash}),
                )
                .await
                .map_err(async_graphql::Error::new)?,
        )?;
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        let mut affected = 0;
        let mut failed = 0;
        for batch in ids.chunks(500) {
            let result = self
                .run(
                    kind,
                    action,
                    batch.to_vec(),
                    from_trash,
                    dest_dir.to_owned(),
                )
                .await?;
            let _ = self.events.send(plain_rs::events::WsEvent::broadcast(
                "CONTENT_CHANGED",
                "{}".into(),
            ));
            affected += result.affected_count;
            failed += result.failed_ids.len();
        }
        if failed != 0 {
            return Err(format!(
                "{}: {failed} media actions failed ({affected} completed)",
                format!("{action:?}").to_uppercase()
            )
            .into());
        }
        Ok(affected)
    }
    pub async fn run(
        &self,
        r#type: DataType,
        action: Action,
        requested: Vec<String>,
        from_trash: bool,
        dest_dir: String,
    ) -> Result<Completed> {
        let invalid = Outcome {
            successful: Vec::new(),
            failed_ids: requested.clone(),
        };
        media_actions::validate(r#type, action, &requested, &invalid)?;
        if requested.is_empty() {
            return Ok(Completed {
                affected_count: 0,
                failed_ids: Vec::new(),
            });
        }
        if action == Action::Move && dest_dir.is_empty() {
            return Err("media destination required".into());
        }
        let host = &self.host;
        let outcome:Outcome=serde_json::from_value(host.call("mediaAction",json!({"type":r#type.kind(),"action":action,"ids":requested,"fromTrash":from_trash,"destDir":dest_dir})).await.map_err(async_graphql::Error::new)?)?;
        media_actions::validate(r#type, action, &requested, &outcome)?;
        let successful = outcome.successful;
        let completed_ids = successful
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let affected = if r#type == DataType::Audio {
            let prefs = self.prefs.clone();
            self.audio
                .run(move |db, engine| {
                    let before = audio_playback::snapshot(db)?;
                    let count = media_actions::cleanup(db, r#type, action, &successful)?;
                    if action != Action::Restore
                        && !before.path.is_empty()
                        && successful.iter().any(|item| item.path == before.path)
                    {
                        audio_commands::command(
                            db,
                            &prefs,
                            engine,
                            audio_commands::Action::Clear,
                            0,
                            1.0,
                        )?;
                    }
                    Ok(count)
                })
                .await?
        } else {
            let db = self.db.clone();
            let index = self.index.clone();
            tokio::task::spawn_blocking(move || {
                if r#type == DataType::Image && action != Action::Restore {
                    index.update_cache(|db| media_actions::cleanup(db, r#type, action, &successful))
                } else {
                    media_actions::cleanup(&db, r#type, action, &successful)
                }
            })
            .await??
        };
        if !completed_ids.is_empty() {
            let _ = self.events.send(plain_rs::events::WsEvent::broadcast(
                plain_rs::events::MEDIA_ACTION_FINISHED,
                json!({"type":r#type.kind(),"action":action,"ids":completed_ids}).to_string(),
            ));
        }
        Ok(Completed {
            affected_count: affected.try_into()?,
            failed_ids: outcome.failed_ids,
        })
    }
}
