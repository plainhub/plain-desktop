//! Public `/graphql` roots that write: file operations, media-library
//! actions and the resumable-upload merge state.
//!
use crate::graphql::resolvers::files::types::File;
use crate::graphql::resolvers::media::public::MediaDataType;
use crate::graphql::types::ActionResult;
use crate::graphql::types::Long;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::host::query as provider_plan;
use crate::host::query::Provider;
use crate::http::auth::public_gate;
use async_graphql::{Context, Enum, Object, SimpleObject};
use plain_rs::db::Db;
use serde_json::{Value, json};
use std::sync::Arc;

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum MergeTaskStatus {
    #[default]
    None,
    Started,
    Merging,
    Done,
    Failed,
}

impl MergeTaskStatus {
    fn parse(value: &str) -> Self {
        match value {
            "STARTED" => Self::Started,
            "MERGING" => Self::Merging,
            "DONE" => Self::Done,
            "FAILED" => Self::Failed,
            _ => Self::None,
        }
    }
}

#[derive(SimpleObject, Clone, Debug, Default)]
pub struct MergeTask {
    pub status: MergeTaskStatus,
    pub value: Option<String>,
    #[graphql(name = "mergedSize")]
    pub merged_size: Option<Long>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct UploadQuery;

#[Object]
impl UploadQuery {
    /// Chunk state for a client-chosen chunk-set id, as `index:byteSize`
    /// strings. An empty list means the upload never started or was
    /// discarded — the caller cannot tell, and does not need to.
    async fn uploaded_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
    ) -> async_graphql::Result<Vec<String>> {
        let base = upload_base(ctx).await?;
        plain_rs::files::uploads::list(&base, &file_id)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }

    /// `NONE` when no merge was ever started for this id, which is what a
    /// client sees before it calls `mergeChunks`.
    async fn merge_status(
        &self,
        ctx: &Context<'_>,
        file_id: String,
    ) -> async_graphql::Result<MergeTask> {
        Ok(merge_task(
            &ctx.data_unchecked::<std::sync::Arc<plain_rs::files::uploads::Runtime>>()
                .status(&file_id),
        ))
    }
}

#[derive(Default)]
pub struct FileOpsMutation;

#[Object]
impl FileOpsMutation {
    /// Counts what was actually removed. A path that was already gone
    /// counts as zero, so the number is the delta rather than the request
    /// size.
    async fn delete_files(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<ActionResult> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        if paths.is_empty() {
            return Ok(ActionResult { affected_count: 0 });
        }
        let tasks = ctx.data::<Arc<crate::services::files::file_tasks::FileTasks>>()?;
        let mut deleted = 0;
        for path in paths {
            if tasks.delete(path).await?.removed {
                deleted += 1;
            }
        }
        Ok(ActionResult {
            affected_count: deleted,
        })
    }

    async fn create_dir(&self, ctx: &Context<'_>, path: String) -> async_graphql::Result<File> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let record = crate::services::files::writes::execute(
            ctx.data::<Arc<Host>>()?,
            plain_rs::files::filesystem::writes::Operation::CreateDirectory { path },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(written_file(record))
    }

    /// `false` rather than an error when the rename is refused: a name that
    /// is empty, a traversal segment, or an existing destination are all
    /// ordinary outcomes for a user typing in a rename box.
    async fn rename_file(
        &self,
        ctx: &Context<'_>,
        path: String,
        name: String,
    ) -> async_graphql::Result<bool> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        Ok(ctx
            .data::<Arc<crate::services::files::file_tasks::FileTasks>>()?
            .rename(path, name)
            .await?
            .is_some())
    }

    async fn write_text_file(
        &self,
        ctx: &Context<'_>,
        path: String,
        content: String,
        overwrite: bool,
    ) -> async_graphql::Result<File> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let record = crate::services::files::writes::execute(
            ctx.data::<Arc<Host>>()?,
            plain_rs::files::filesystem::writes::Operation::WriteText {
                path,
                content,
                overwrite,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(written_file(record))
    }

    async fn copy_file(
        &self,
        ctx: &Context<'_>,
        src: String,
        dst: String,
        overwrite: bool,
    ) -> async_graphql::Result<bool> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        transfer(ctx, "COPY", src, dst, overwrite).await
    }

    async fn move_file(
        &self,
        ctx: &Context<'_>,
        src: String,
        dst: String,
        overwrite: bool,
    ) -> async_graphql::Result<bool> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        transfer(ctx, "MOVE", src, dst, overwrite).await
    }

    async fn delete_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
    ) -> async_graphql::Result<bool> {
        let base = upload_base(ctx).await?;
        ctx.data_unchecked::<std::sync::Arc<plain_rs::files::uploads::Runtime>>()
            .delete(&base, &file_id)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }

    /// Starts the merge and returns immediately; completion arrives over the
    /// websocket, and `mergeStatus` is the polling fallback for a lost event.
    async fn merge_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
        total_chunks: i32,
        path: String,
        replace: bool,
        total_size: crate::graphql::types::Long,
    ) -> async_graphql::Result<MergeTask> {
        start_merge(
            ctx,
            file_id,
            total_chunks,
            total_size.0,
            plain_rs::files::uploads::Kind::File {
                path: std::path::PathBuf::from(path),
                replace,
            },
        )
        .await
    }

    async fn merge_app_file_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
        total_chunks: i32,
        file_name: String,
        total_size: crate::graphql::types::Long,
    ) -> async_graphql::Result<MergeTask> {
        start_merge(
            ctx,
            file_id,
            total_chunks,
            total_size.0,
            plain_rs::files::uploads::Kind::AppFile { name: file_name },
        )
        .await
    }
}

#[derive(Default)]
pub struct MediaActionMutation;

#[Object]
impl MediaActionMutation {
    /// A blank query would match the whole library, and a bulk delete is
    /// not something a client should be able to aim at everything by
    /// accident — it is refused before the platform is asked.
    async fn delete_media_items(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        require_explicit_query(&query)?;
        require_narrowing_media(ctx, r#type, &query).await?;
        let affected = media_action(ctx, "delete", r#type, &query, Value::Null).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    async fn trash_media_items(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        require_explicit_query(&query)?;
        require_narrowing_media(ctx, r#type, &query).await?;
        let affected = media_action(ctx, "trash", r#type, &query, Value::Null).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    async fn restore_media_items(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        require_explicit_query(&query)?;
        require_narrowing_media(ctx, r#type, &query).await?;
        let affected = media_action(ctx, "restore", r#type, &query, Value::Null).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    /// The only media action that is also a storage write, so it is the
    /// only one that gates — the others just move rows between the library
    /// and the trash.
    async fn move_media_items(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
        query: String,
        dest_dir: String,
    ) -> async_graphql::Result<ActionResult> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        require_explicit_query(&query)?;
        require_narrowing_media(ctx, r#type, &query).await?;
        let affected = media_action(ctx, "move", r#type, &query, json!(dest_dir)).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }
}

fn require_explicit_query(query: &str) -> async_graphql::Result<()> {
    if query.trim().is_empty() {
        return Err(async_graphql::Error::new("explicit query required"));
    }
    Ok(())
}

/// Refuses a media action whose query builds no clause, which would resolve to
/// every row of the library. Blank is already the caller's business; this is
/// the other half — a query can be non-empty and still name nothing that
/// narrows, because the fields it carries are ones this provider does not act
/// on.
async fn require_narrowing_media(
    ctx: &Context<'_>,
    r#type: MediaDataType,
    query: &str,
) -> async_graphql::Result<()> {
    let provider = match r#type {
        MediaDataType::AUDIO => Provider::Audio,
        MediaDataType::VIDEO => Provider::Video,
        MediaDataType::IMAGE => Provider::Image,
        MediaDataType::DOC => Provider::Doc,
    };
    provider_plan::require_narrowing(
        ctx.data_unchecked::<Arc<Db>>(),
        ctx.data_unchecked::<Arc<Host>>(),
        provider,
        query,
    )
    .await
    .map_err(|error| async_graphql::Error::new(error.to_string()))
}

async fn transfer(
    ctx: &Context<'_>,
    kind: &str,
    src: String,
    dst: String,
    overwrite: bool,
) -> async_graphql::Result<bool> {
    let kind = match kind {
        "COPY" => plain_rs::files::tasks::FileTaskType::Copy,
        "MOVE" => plain_rs::files::tasks::FileTaskType::Move,
        _ => return Err("invalid file task type".into()),
    };
    Ok(ctx
        .data::<Arc<crate::services::files::file_tasks::FileTasks>>()?
        .execute(
            kind,
            vec![plain_rs::files::tasks::FileTaskOp {
                src,
                dst,
                overwrite,
            }],
        )
        .await?)
}

async fn media_action(
    ctx: &Context<'_>,
    action: &str,
    r#type: MediaDataType,
    query: &str,
    dest: Value,
) -> async_graphql::Result<i32> {
    use plain_rs::media::actions::Action;
    let action = match action {
        "delete" => Action::Delete,
        "trash" => Action::Trash,
        "restore" => Action::Restore,
        "move" => Action::Move,
        _ => return Err("invalid media action".into()),
    };
    let data_type = match r#type {
        MediaDataType::AUDIO => plain_rs::enums::DataType::Audio,
        MediaDataType::VIDEO => plain_rs::enums::DataType::Video,
        MediaDataType::IMAGE => plain_rs::enums::DataType::Image,
        MediaDataType::DOC => plain_rs::enums::DataType::Doc,
    };
    ctx.data::<Arc<crate::services::files::media_actions::MediaActions>>()?
        .run_query(data_type, action, query, dest.as_str().unwrap_or_default())
        .await
}

fn written_file(record: plain_rs::files::filesystem::record::FileRecord) -> File {
    File {
        media_id: (!record.media_id.is_empty()).then(|| record.media_id.into()),
        name: record.name,
        path: record.path,
        created_at: record
            .created_at
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(crate::graphql::types::Instant),
        updated_at: crate::graphql::types::Instant(
            chrono::DateTime::from_timestamp_millis(record.updated_at).unwrap_or_default(),
        ),
        size: Long(record.size),
        is_dir: record.is_dir,
        child_count: record.child_count,
    }
}

fn merge_task(value: &Value) -> MergeTask {
    MergeTask {
        status: MergeTaskStatus::parse(&text(value, "status")),
        value: value["value"].as_str().map(str::to_string),
        merged_size: value["mergedSize"].as_i64().map(Long),
        error: value["error"].as_str().map(str::to_string),
    }
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<plain_rs::prefs::Prefs> {
    ctx.data_unchecked::<Arc<plain_rs::prefs::Prefs>>()
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/files/operations.rs"]
mod tests;

async fn upload_base(ctx: &Context<'_>) -> async_graphql::Result<std::path::PathBuf> {
    Ok(ctx
        .data::<Arc<crate::services::files::upload_chunks::UploadChunks>>()?
        .directory()
        .to_path_buf())
}
async fn start_merge(
    ctx: &Context<'_>,
    id: String,
    count: i32,
    size: i64,
    kind: plain_rs::files::uploads::Kind,
) -> async_graphql::Result<MergeTask> {
    let base = upload_base(ctx).await?;
    let store = ctx
        .data_unchecked::<std::sync::Arc<plain_rs::files::app_files::FileStore>>()
        .clone();
    let runtime = ctx
        .data_unchecked::<std::sync::Arc<plain_rs::files::uploads::Runtime>>()
        .clone();
    let events = ctx
        .data_unchecked::<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>()
        .clone();
    let value = runtime
        .start(store, base, id, count, size, kind, events)
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(merge_task(&value))
}
