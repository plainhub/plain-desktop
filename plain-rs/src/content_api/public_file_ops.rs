//! Public `/graphql` roots that write: file operations, media-library
//! actions and the resumable-upload merge state.
//!
//! The work itself is platform — deleting a tree, moving a file, running a
//! copy task, decoding a media-store id — so every mutation here forwards
//! to the host. What Rust owns is the contract shape, the permission gate,
//! and the two rules the platform relies on the caller to enforce: a blank
//! query never means "everything", and a bulk delete never runs unguarded.

use super::host::Host;
use super::provider_plan::{self, Provider};
use super::public_facts::{integer, text};
use super::public_gate;
use super::public_media::MediaDataType;
use crate::content_types::{ActionResult, File, Long};
use crate::db::Db;
use async_graphql::{Context, Enum, Object, SimpleObject};
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
        let base=upload_base(ctx).await?;
        crate::uploads::list(&base,&file_id).await.map_err(|error|async_graphql::Error::new(error.to_string()))

    }

    /// `NONE` when no merge was ever started for this id, which is what a
    /// client sees before it calls `mergeChunks`.
    async fn merge_status(
        &self,
        ctx: &Context<'_>,
        file_id: String,
    ) -> async_graphql::Result<MergeTask> {
        Ok(merge_task(&ctx.data_unchecked::<std::sync::Arc<crate::uploads::Runtime>>().status(&file_id)))

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
        public_gate::require(prefs(ctx), &[STORAGE])?;
        if paths.is_empty() {
            return Ok(ActionResult { affected_count: 0 });
        }
        let deleted = host_call(ctx, "systemDeleteFiles", json!({ "paths": paths })).await?;
        Ok(ActionResult {
            affected_count: deleted.as_i64().unwrap_or_default() as i32,
        })
    }

    async fn create_dir(&self, ctx: &Context<'_>, path: String) -> async_graphql::Result<File> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let facts = host_call(ctx, "systemCreateDir", json!({ "path": path })).await?;
        Ok(file(&facts))
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
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let renamed = host_call(
            ctx,
            "systemRenameFile",
            json!({ "path": path, "name": name }),
        )
        .await?;
        Ok(renamed.as_bool().unwrap_or(false))
    }

    async fn write_text_file(
        &self,
        ctx: &Context<'_>,
        path: String,
        content: String,
        overwrite: bool,
    ) -> async_graphql::Result<File> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let facts = host_call(
            ctx,
            "systemWriteTextFile",
            json!({ "path": path, "content": content, "overwrite": overwrite }),
        )
        .await?;
        Ok(file(&facts))
    }

    async fn copy_file(
        &self,
        ctx: &Context<'_>,
        src: String,
        dst: String,
        overwrite: bool,
    ) -> async_graphql::Result<bool> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        transfer(ctx, "COPY", src, dst, overwrite).await
    }

    async fn move_file(
        &self,
        ctx: &Context<'_>,
        src: String,
        dst: String,
        overwrite: bool,
    ) -> async_graphql::Result<bool> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        transfer(ctx, "MOVE", src, dst, overwrite).await
    }

    async fn delete_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
    ) -> async_graphql::Result<bool> {
        let base=upload_base(ctx).await?;
        ctx.data_unchecked::<std::sync::Arc<crate::uploads::Runtime>>().delete(&base,&file_id).await.map_err(|error|async_graphql::Error::new(error.to_string()))

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
        total_size: crate::content_types::Long,
    ) -> async_graphql::Result<MergeTask> {
        start_merge(ctx,file_id,total_chunks,total_size.0,crate::uploads::Kind::File {path:std::path::PathBuf::from(path),replace}).await

    }

    async fn merge_app_file_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
        total_chunks: i32,
        file_name: String,
        total_size: crate::content_types::Long,
    ) -> async_graphql::Result<MergeTask> {
        start_merge(ctx,file_id,total_chunks,total_size.0,crate::uploads::Kind::AppFile {name:file_name}).await

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
        public_gate::require(prefs(ctx), &[STORAGE])?;
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
        MediaDataType::Audio => Provider::Audio,
        MediaDataType::Video => Provider::Video,
        MediaDataType::Image => Provider::Image,
        MediaDataType::Doc => Provider::Doc,
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
    let moved = host_call(
        ctx,
        "systemTransferFile",
        json!({ "type": kind, "src": src, "dst": dst, "overwrite": overwrite }),
    )
    .await?;
    Ok(moved.as_bool().unwrap_or(false))
}

async fn media_action(
    ctx: &Context<'_>,
    action: &str,
    r#type: MediaDataType,
    query: &str,
    dest: Value,
) -> async_graphql::Result<i32> {
    let affected = host_call(
        ctx,
        "systemMediaAction",
        json!({ "action": action, "dataType": media_kind(r#type), "query": query, "destDir": dest }),
    )
    .await?;
    Ok(affected.as_i64().unwrap_or_default() as i32)
}

/// `MediaDataType` is the contract's spelling and also the platform's
/// `DataType` enum name, so the mapping is the identity.
fn media_kind(r#type: MediaDataType) -> &'static str {
    match r#type {
        MediaDataType::Audio => "AUDIO",
        MediaDataType::Video => "VIDEO",
        MediaDataType::Image => "IMAGE",
        MediaDataType::Doc => "DOC",
    }
}

fn file(value: &Value) -> File {
    File {
        media_id: value["mediaId"]
            .as_str()
            .filter(|media| !media.is_empty())
            .map(|media| async_graphql::ID::from(media)),
        name: text(value, "name"),
        path: text(value, "path"),
        created_at: super::public_facts::optional_instant(value, "createdAt"),
        updated_at: super::public_facts::instant(value, "updatedAt"),
        size: Long(integer(value, "size")),
        is_dir: super::public_facts::flag(value, "isDir"),
        child_count: integer(value, "childCount") as i32,
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

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<crate::prefs::Prefs> {
    ctx.data_unchecked::<Arc<crate::prefs::Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_file_ops.rs"]
mod tests;

async fn upload_base(ctx:&Context<'_>)->async_graphql::Result<std::path::PathBuf> {
    let facts=host_call(ctx,"uploadTmpDirFacts",json!({})).await?;
    Ok(std::path::PathBuf::from(text(&facts,"path")))
}
async fn start_merge(ctx:&Context<'_>,id:String,count:i32,size:i64,kind:crate::uploads::Kind)->async_graphql::Result<MergeTask> {
    let base=upload_base(ctx).await?;
    let store=ctx.data_unchecked::<std::sync::Arc<crate::app_files::FileStore>>().clone();
    let runtime=ctx.data_unchecked::<std::sync::Arc<crate::uploads::Runtime>>().clone();
    let events=ctx.data_unchecked::<tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>>().clone();
    let value=runtime.start(store,base,id,count,size,kind,events).await.map_err(|error|async_graphql::Error::new(error.to_string()))?;
    Ok(merge_task(&value))
}
