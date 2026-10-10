use super::host::Host;
use crate::library::{
    LibraryError, LibraryResult,
    audio_queue::{AudioTrack, LibraryTracks},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct HostLibrary {
    host: Arc<Host>,
    runtime: tokio::runtime::Handle,
}
impl HostLibrary {
    pub fn new(host: Arc<Host>) -> Self {
        Self {
            host,
            runtime: tokio::runtime::Handle::current(),
        }
    }
    fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> LibraryResult<T> {
        let result = self
            .runtime
            .block_on(self.host.call(method, params))
            .map_err(LibraryError::Other)?;
        serde_json::from_value(result)
            .map_err(|e| LibraryError::Other(format!("invalid audio provider response: {e}")))
    }
}
impl LibraryTracks for HostLibrary {
    fn library_count(&mut self) -> LibraryResult<usize> {
        self.call("audioLibraryCount", json!({}))
    }
    fn library_path_at(&mut self, offset: usize, sort_by: &str) -> LibraryResult<Option<String>> {
        self.call(
            "audioLibraryPath",
            json!({"offset":offset,"sortBy":sort_by}),
        )
    }
    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> LibraryResult<Vec<AudioTrack>> {
        self.call(
            "audioLibraryPage",
            json!({"offset":offset,"limit":limit,"sortBy":sort_by}),
        )
    }
    fn library_locate(&mut self, path: &str, sort_by: &str) -> LibraryResult<i64> {
        self.call("audioLibraryLocate", json!({"path":path,"sortBy":sort_by}))
    }
    fn library_contains(&mut self, path: &str) -> LibraryResult<bool> {
        self.call("audioLibraryContains", json!({"path":path}))
    }
}

impl crate::library::audio_commands::Engine for HostLibrary {
    fn metadata(&mut self, path: &str) -> LibraryResult<AudioTrack> {
        self.call("audioMetadata", json!({"path":path}))
    }
    fn execute(
        &mut self,
        command: &crate::library::audio_commands::EngineCommand,
    ) -> LibraryResult<crate::library::audio_commands::EngineReport> {
        self.call(
            "audioEngineCommand",
            serde_json::to_value(command).map_err(|e| LibraryError::Other(e.to_string()))?,
        )
    }
}
