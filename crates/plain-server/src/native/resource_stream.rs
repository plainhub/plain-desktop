use crate::host::bridge::Host;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

pub(crate) enum ResourceFrame {
    Request(String),
    Bytes(Vec<u8>),
    End,
    Error,
}

pub struct NativeResourceStream {
    pub(crate) host: Arc<Host>,
    pub(crate) generation: u64,
    pub(crate) outgoing: tokio::sync::Mutex<mpsc::Receiver<ResourceFrame>>,
    pub(crate) incoming: mpsc::Sender<ResourceFrame>,
    pub(crate) stop: watch::Receiver<bool>,
    pub(crate) completed: tokio::sync::Mutex<bool>,
}

impl NativeResourceStream {
    pub async fn next_request(&self) -> Result<Option<String>, String> {
        let mut connection = self.host.connection_changes();
        let mut stop = self.stop.clone();
        if *stop.borrow() || !self.host.is_current(self.generation) {
            return Err("resource stream closed".into());
        }
        let mut outgoing = tokio::select! {
            _ = stop.changed() => return Err("resource stream closed".into()),
            _ = connection.changed() => return Err("host disconnected".into()),
            outgoing = self.outgoing.lock() => outgoing,
        };
        tokio::select! {
            _ = stop.changed() => Err("resource stream closed".into()),
            _ = connection.changed() => Err("host disconnected".into()),
            value = outgoing.recv() => match value {
                Some(ResourceFrame::Request(value)) => Ok(Some(value)),
                None => Ok(None),
                _ => Err("invalid resource request".into()),
            },
        }
    }

    pub async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > 64 * 1024 {
            return Err("invalid resource chunk size".into());
        }
        self.send(ResourceFrame::Bytes(bytes), false).await
    }

    pub async fn finish(&self) -> Result<(), String> {
        self.send(ResourceFrame::End, true).await
    }

    pub async fn fail(&self) -> Result<(), String> {
        self.send(ResourceFrame::Error, true).await
    }

    async fn send(&self, message: ResourceFrame, terminal: bool) -> Result<(), String> {
        let mut connection = self.host.connection_changes();
        let mut stop = self.stop.clone();
        if *stop.borrow() || !self.host.is_current(self.generation) {
            return Err("resource stream closed".into());
        }
        let mut completed = tokio::select! {
            _ = stop.changed() => return Err("resource stream closed".into()),
            _ = connection.changed() => return Err("host disconnected".into()),
            completed = self.completed.lock() => completed,
        };
        if *completed {
            return Err("resource stream finished".into());
        }
        let result = tokio::select! {
            _ = stop.changed() => Err("resource stream closed".into()),
            _ = connection.changed() => Err("host disconnected".into()),
            result = self.incoming.send(message) => result.map_err(|_| "resource stream consumer closed".into()),
        };
        if result.is_ok() && terminal {
            *completed = true;
        }
        result
    }
}
