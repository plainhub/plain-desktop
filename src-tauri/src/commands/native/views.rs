use plain_server::server::ContentServer;
use plain_rs::host::request_scope::NativeRequestScope;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::Manager;

#[derive(Default)]
pub struct NativeViews {
    scopes: Mutex<HashMap<String, NativeRequestScope>>,
}

impl NativeViews {
    pub fn scope(&self, label: &str) -> NativeRequestScope {
        self.scopes
            .lock()
            .unwrap()
            .entry(label.into())
            .or_default()
            .clone()
    }

    pub fn existing_scope(&self, label: &str, id: &str) -> Result<NativeRequestScope, String> {
        self.scopes
            .lock()
            .unwrap()
            .get(label)
            .filter(|scope| scope.id() == id && !scope.is_closed())
            .cloned()
            .ok_or_else(|| "Native request scope closed".into())
    }

    fn replace(&self, label: &str, reset: bool) -> Option<NativeRequestScope> {
        let mut scopes = self.scopes.lock().unwrap();
        let previous = scopes.remove(label);
        if let Some(scope) = &previous {
            scope.close();
        }
        if reset {
            scopes.insert(label.into(), NativeRequestScope::default());
        }
        previous
    }
}

pub fn page_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>, label: &str, reset: bool) {
    if let Some(callbacks) = app.try_state::<super::callbacks::HostCallbacks>() {
        callbacks.remove_view(label);
    }
    let Some(views) = app.try_state::<NativeViews>() else {
        return;
    };
    let Some(previous) = views.replace(label, reset) else {
        return;
    };
    if let Some(resources) =
        app.try_state::<Arc<crate::native_resource_protocol::ResourceRegistry>>()
    {
        resources.close_owner(previous.id());
    }
    if let Some(writers) = app.try_state::<super::uploads::UploadWriters>() {
        writers.abort_owner(previous.id());
    }
    let Some(core) = app.try_state::<Arc<ContentServer>>() else {
        return;
    };
    let core = core.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = core.close_native_scope(&previous).await {
            log::warn!("Native view cleanup: {error}");
        }
    });
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/native/views.rs"]
mod tests;
