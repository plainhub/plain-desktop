use crate::{
    content_api::file_tasks::FileTasks,
    content_types::{Instant, Long},
    filesystem::tasks::{CompletedOp, FileTask, FileTaskOp, FileTaskStatus, FileTaskType},
};
use async_graphql::{Context, ID, InputObject, Object, Result, SimpleObject};
use std::sync::Arc;
#[derive(InputObject)]
struct FileHostTaskOpInput {
    src: String,
    dst: String,
    overwrite: bool,
}
#[derive(SimpleObject)]
struct FileHostCompletedOp {
    src: String,
    dst: String,
}
impl From<CompletedOp> for FileHostCompletedOp {
    fn from(op: CompletedOp) -> Self {
        Self {
            src: op.src,
            dst: op.dst,
        }
    }
}
#[derive(SimpleObject)]
struct FileHostTask {
    id: ID,
    client_id: String,
    r#type: FileTaskType,
    title: String,
    status: FileTaskStatus,
    error: String,
    total_bytes: Long,
    done_bytes: Long,
    total_items: Long,
    done_items: Long,
    created_at: Instant,
    updated_at: Instant,
    completed_ops: Vec<FileHostCompletedOp>,
}
impl From<FileTask> for FileHostTask {
    fn from(t: FileTask) -> Self {
        Self {
            id: ID(t.id),
            client_id: t.client_id,
            r#type: t.kind,
            title: t.title,
            status: t.status,
            error: t.error,
            total_bytes: Long(t.total_bytes),
            done_bytes: Long(t.done_bytes),
            total_items: Long(t.total_items),
            done_items: Long(t.done_items),
            created_at: Instant(t.created_at),
            updated_at: Instant(t.updated_at),
            completed_ops: t.completed_ops.into_iter().map(Into::into).collect(),
        }
    }
}
#[derive(Default)]
pub struct FileTaskQuery;
#[Object]
impl FileTaskQuery {
    async fn file_host_task_record(
        &self,
        ctx: &Context<'_>,
        client_id: String,
        id: ID,
    ) -> Result<Option<FileHostTask>> {
        Ok(ctx
            .data::<Arc<FileTasks>>()?
            .list(client_id)
            .await?
            .into_iter()
            .find(|task| task.id == id.as_str())
            .map(Into::into))
    }
    async fn file_host_tasks(
        &self,
        ctx: &Context<'_>,
        client_id: String,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Result<Vec<FileHostTask>> {
        if offset < 0 || limit < 0 {
            return Err("invalid pagination".into());
        }
        let text = query.to_lowercase();
        Ok(ctx
            .data::<Arc<FileTasks>>()?
            .list(client_id)
            .await?
            .into_iter()
            .filter(|t| t.title.to_lowercase().contains(&text))
            .skip(offset as usize)
            .take(limit as usize)
            .map(Into::into)
            .collect())
    }
}
#[derive(Default)]
pub struct FileTaskMutation;
#[Object]
impl FileTaskMutation {
    async fn remove_file_host_task(
        &self,
        ctx: &Context<'_>,
        client_id: String,
        id: ID,
    ) -> Result<bool> {
        Ok(ctx
            .data::<Arc<FileTasks>>()?
            .remove(client_id, id.to_string())
            .await?)
    }
    async fn create_file_host_task(
        &self,
        ctx: &Context<'_>,
        client_id: String,
        r#type: FileTaskType,
        title: String,
        ops: Vec<FileHostTaskOpInput>,
    ) -> Result<FileHostTask> {
        Ok(ctx
            .data::<Arc<FileTasks>>()?
            .create(
                &client_id,
                r#type,
                &title,
                ops.into_iter()
                    .map(|op| FileTaskOp {
                        src: op.src,
                        dst: op.dst,
                        overwrite: op.overwrite,
                    })
                    .collect(),
            )
            .await?
            .into())
    }
}
