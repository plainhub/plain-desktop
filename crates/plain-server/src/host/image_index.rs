use crate::host::bridge::Host;
use plain_rs::error::DomainError;
use plain_rs::error::DomainResult;
use plain_rs::media::images::indexing::Embedded;
use plain_rs::media::images::indexing::Image;
use plain_rs::media::images::indexing::Page;
use plain_rs::media::images::indexing::Provider;
use plain_rs::media::images::indexing::Snapshot;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
pub(crate) struct HostProvider {
    job_id: String,
    host: Arc<Host>,
    runtime: tokio::runtime::Handle,
    ended: bool,
    encoder: Option<Arc<dyn plain_inference::embedding::ImageEncoder>>,
}
impl HostProvider {
    pub fn new(
        host: Arc<Host>,
        job_id: String,
        encoder: Option<Arc<dyn plain_inference::embedding::ImageEncoder>>,
    ) -> Self {
        Self {
            host,
            job_id,
            runtime: tokio::runtime::Handle::current(),
            ended: false,
            encoder,
        }
    }

    pub fn finish(&mut self) -> DomainResult<()> {
        if self.ended {
            return Ok(());
        }
        let ended: bool = self.call("imageIndexEnd", json!({}))?;
        if !ended {
            return Err(DomainError::Other(
                "image index engine cleanup failed".into(),
            ));
        }
        if let Some(encoder) = &self.encoder {
            encoder.release();
        }
        self.ended = true;
        Ok(())
    }

    fn call<T: DeserializeOwned>(&self, method: &str, mut params: Value) -> DomainResult<T> {
        params
            .as_object_mut()
            .unwrap()
            .insert("jobId".into(), json!(self.job_id));
        let value = self
            .runtime
            .block_on(self.host.call(method, params))
            .map_err(DomainError::Other)?;
        serde_json::from_value(value)
            .map_err(|e| DomainError::Other(format!("invalid image index host response: {e}")))
    }
}
impl Provider for HostProvider {
    fn resolve(&mut self, revision: &str, ids: &[String]) -> DomainResult<Vec<Image>> {
        self.call("imageIndexResolve", json!({"revision":revision,"ids":ids}))
    }
    fn snapshot(&mut self) -> DomainResult<Snapshot> {
        self.call("imageIndexBegin", json!({}))
    }
    fn page(&mut self, revision: &str, cursor: &str, limit: usize) -> DomainResult<Page> {
        self.call(
            "imageIndexPage",
            json!({"revision":revision,"cursor":cursor,"limit":limit}),
        )
    }
    fn embed(&mut self, revision: &str, items: &[Image]) -> DomainResult<Embedded> {
        self.verify(revision)?;
        let encoder = self
            .encoder
            .as_ref()
            .ok_or_else(|| DomainError::Other("Image encoder unavailable".into()))?;
        let mut embedded = Embedded {
            items: Vec::new(),
            skipped_ids: Vec::new(),
        };
        for item in items {
            let vector = if item.path.starts_with("ph://") {
                None
            } else {
                encoder
                    .image(std::path::Path::new(&item.path))
                    .map_err(DomainError::Other)?
            };
            let vector = if vector.is_none() {
                let decoded: Option<String> =
                    self.call("imageIndexDecode", json!({"path":item.path,"id":item.id}))?;
                if let Some(path) = decoded {
                    let result = encoder
                        .image(std::path::Path::new(&path))
                        .map_err(DomainError::Other);
                    let released: bool = self.call("imageIndexRelease", json!({"path":path}))?;
                    if !released {
                        return Err(DomainError::Other(
                            "Image decode resource cleanup failed".into(),
                        ));
                    }
                    result?
                } else {
                    None
                }
            } else {
                vector
            };
            match vector {
                Some(vector) => {
                    embedded
                        .items
                        .push(plain_rs::media::images::embeddings::EmbeddingInput {
                            id: item.id.clone(),
                            path: item.path.clone(),
                            embedding_base64: plain_rs::utils::base64::base64_encode(
                                &vector
                                    .iter()
                                    .flat_map(|v| v.to_be_bytes())
                                    .collect::<Vec<_>>(),
                            ),
                        })
                }
                None => embedded.skipped_ids.push(item.id.clone()),
            }
        }
        self.verify(revision)?;
        Ok(embedded)
    }
    fn verify(&mut self, revision: &str) -> DomainResult<()> {
        let verified: bool = self.call("imageIndexVerify", json!({"revision":revision}))?;
        if !verified {
            return Err(DomainError::Other(
                "image catalog verification failed".into(),
            ));
        }
        Ok(())
    }
}
impl Drop for HostProvider {
    fn drop(&mut self) {
        if let Some(encoder) = &self.encoder {
            encoder.release();
        }
        if self.ended {
            return;
        }
        let _ = self.runtime.block_on(
            self.host
                .call("imageIndexEnd", json!({"jobId":self.job_id})),
        );
    }
}
