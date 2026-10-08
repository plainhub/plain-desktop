use async_graphql::{Context, Object, Result};
use std::sync::Arc;
use crate::api::context::AppCtx;
use super::types::{Long, MergeTask, MergeTaskStatus};

#[derive(Default)]
pub struct FileUploadQuery;
#[derive(Default)]
pub struct FileUploadMutation;
fn task(value: serde_json::Value)->MergeTask {
    MergeTask {
        status:match value["status"].as_str() {Some("STARTED")=>MergeTaskStatus::Started,Some("MERGING")=>MergeTaskStatus::Merging,Some("DONE")=>MergeTaskStatus::Done,Some("FAILED")=>MergeTaskStatus::Failed,_=>MergeTaskStatus::None},
        value:value["value"].as_str().map(str::to_owned),merged_size:value["mergedSize"].as_i64().map(Long),error:value["error"].as_str().map(str::to_owned)
    }
}
#[Object]
impl FileUploadQuery {
    async fn uploaded_chunks(&self,ctx:&Context<'_>,file_id:String)->Result<Vec<String>> {
        let c=ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(crate::uploads::list(&c.data_dir.join("upload_tmp"),&file_id).await?)
    }
    async fn merge_status(&self,ctx:&Context<'_>,file_id:String)->MergeTask {
        task(ctx.data_unchecked::<Arc<AppCtx>>().uploads.status(&file_id))
    }
}
#[Object]
impl FileUploadMutation {
    async fn delete_chunks(&self,ctx:&Context<'_>,file_id:String)->Result<bool> {
        let c=ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(c.uploads.delete(&c.data_dir.join("upload_tmp"),&file_id).await?)
    }
    async fn merge_chunks(&self,ctx:&Context<'_>,file_id:String,total_chunks:i32,path:String,replace:bool,total_size:Long)->Result<MergeTask> {
        start(ctx,file_id,total_chunks,total_size.0,crate::uploads::Kind::File {path:path.into(),replace}).await
    }
    async fn merge_app_file_chunks(&self,ctx:&Context<'_>,file_id:String,total_chunks:i32,file_name:String,total_size:Long)->Result<MergeTask> {
        start(ctx,file_id,total_chunks,total_size.0,crate::uploads::Kind::AppFile {name:file_name}).await
    }
}
async fn start(ctx:&Context<'_>,id:String,count:i32,size:i64,kind:crate::uploads::Kind)->Result<MergeTask> {
    let c=ctx.data_unchecked::<Arc<AppCtx>>();
    let store=crate::app_files::FileStore::new(c.db.clone(),c.data_dir.clone());
    Ok(task(c.uploads.start(store,c.data_dir.join("upload_tmp"),id,count,size,kind,c.event_tx.clone()).await?))
}

#[cfg(test)]
#[path="../../../tests/unit/api/schema/file_upload.rs"]
mod tests;
