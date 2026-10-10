use crate::{
    content_api::{audio::Audio, host::Host, image_index::ImageIndex},
    db::Db,
    enums::DataType,
    library::{
        audio_commands, audio_playback,
        media_actions::{self, Action, Outcome},
    },
    prefs::Prefs,
};
use async_graphql::{Context, ID, Object, Result, SimpleObject};
use serde_json::json;
use std::sync::Arc;
#[derive(SimpleObject)]
struct MediaHostActionResult {
    affected_count: i32,
    failed_ids: Vec<ID>,
}
#[derive(Default)]
pub struct MediaActionMutation;
#[Object]
impl MediaActionMutation {
    async fn media_host_action(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        action: Action,
        ids: Vec<ID>,
        from_trash: bool,
        dest_dir: String,
    ) -> Result<MediaHostActionResult> {
        let requested = ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let invalid = Outcome {
            successful: Vec::new(),
            failed_ids: requested.clone(),
        };
        media_actions::validate(r#type, action, &requested, &invalid)?;
        if requested.is_empty() {
            return Ok(MediaHostActionResult {
                affected_count: 0,
                failed_ids: Vec::new(),
            });
        }
        if action == Action::Move && dest_dir.is_empty() {
            return Err("media destination required".into());
        }
        let host = ctx.data::<Arc<Host>>()?;
        let outcome:Outcome=serde_json::from_value(host.call("mediaAction",json!({"type":r#type.kind(),"action":action,"ids":requested,"fromTrash":from_trash,"destDir":dest_dir})).await.map_err(async_graphql::Error::new)?)?;
        media_actions::validate(r#type, action, &requested, &outcome)?;
        let successful = outcome.successful;
        let affected = if r#type == DataType::Audio {
            let prefs = ctx.data::<Arc<Prefs>>()?.clone();
            ctx.data::<Arc<Audio>>()?
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
            let db = ctx.data::<Arc<Db>>()?.clone();
            let index = ctx.data::<Arc<ImageIndex>>()?.clone();
            tokio::task::spawn_blocking(move || {
                if r#type == DataType::Image && action != Action::Restore {
                    index.update_cache(|db| media_actions::cleanup(db, r#type, action, &successful))
                } else {
                    media_actions::cleanup(&db, r#type, action, &successful)
                }
            })
            .await??
        };
        Ok(MediaHostActionResult {
            affected_count: affected.try_into()?,
            failed_ids: outcome.failed_ids.into_iter().map(ID).collect(),
        })
    }
}
