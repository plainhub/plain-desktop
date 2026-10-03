use super::host::Host;
use crate::library::{
    LibraryError, LibraryResult,
    image_indexing::{Embedded, Image, Page, Provider, Snapshot},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
pub(super) struct HostProvider {
    job_id: String,
    host: Arc<Host>,
    runtime: tokio::runtime::Handle,
    ended: bool,
}
impl HostProvider {
    pub fn new(host: Arc<Host>, job_id: String) -> Self {
        Self {
            host,
            job_id,
            runtime: tokio::runtime::Handle::current(),
            ended: false,
        }
    }

    pub fn finish(&mut self) -> LibraryResult<()> {
        if self.ended {
            return Ok(());
        }
        let ended: bool = self.call("imageIndexEnd", json!({}))?;
        if !ended {
            return Err(LibraryError::Other(
                "image index engine cleanup failed".into(),
            ));
        }
        self.ended = true;
        Ok(())
    }

    fn call<T: DeserializeOwned>(&self, method: &str, mut params: Value) -> LibraryResult<T> {
        params
            .as_object_mut()
            .unwrap()
            .insert("jobId".into(), json!(self.job_id));
        let value = self
            .runtime
            .block_on(self.host.call(method, params))
            .map_err(LibraryError::Other)?;
        serde_json::from_value(value)
            .map_err(|e| LibraryError::Other(format!("invalid image index host response: {e}")))
    }
}
impl Provider for HostProvider {
    fn resolve(&mut self, revision: &str, ids: &[String]) -> LibraryResult<Vec<Image>> {
        self.call("imageIndexResolve", json!({"revision":revision,"ids":ids}))
    }
    fn snapshot(&mut self) -> LibraryResult<Snapshot> {
        self.call("imageIndexBegin", json!({}))
    }
    fn page(&mut self, revision: &str, cursor: &str, limit: usize) -> LibraryResult<Page> {
        self.call(
            "imageIndexPage",
            json!({"revision":revision,"cursor":cursor,"limit":limit}),
        )
    }
    fn embed(&mut self, revision: &str, items: &[Image]) -> LibraryResult<Embedded> {
        self.call(
            "imageIndexEmbed",
            json!({"revision":revision,"items":items}),
        )
    }
    fn verify(&mut self, revision: &str) -> LibraryResult<()> {
        let verified: bool = self.call("imageIndexVerify", json!({"revision":revision}))?;
        if !verified {
            return Err(LibraryError::Other(
                "image catalog verification failed".into(),
            ));
        }
        Ok(())
    }
}
impl Drop for HostProvider {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        let _ = self.runtime.block_on(
            self.host
                .call("imageIndexEnd", json!({"jobId":self.job_id})),
        );
    }
}
