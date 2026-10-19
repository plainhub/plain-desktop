use std::ffi::OsString;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone)]
pub(crate) struct Directories {
    pub data: PathBuf,
    pub cache: PathBuf,
}

#[derive(Clone)]
pub(crate) struct Options {
    pub directories: Option<Directories>,
    pub private_webview: bool,
}

impl Options {
    pub fn read() -> Result<Self, String> {
        Self::parse(
            std::env::var_os("PLAINAPP_DATA_DIR"),
            std::env::var_os("PLAINAPP_CACHE_DIR"),
            std::env::var_os("PLAINAPP_PRIVATE_WEBVIEW"),
        )
    }

    fn parse(
        data: Option<OsString>,
        cache: Option<OsString>,
        private: Option<OsString>,
    ) -> Result<Self, String> {
        let directories = match (data, cache) {
            (None, None) => None,
            (Some(data), Some(cache)) => Some(Directories {
                data: data.into(),
                cache: cache.into(),
            }),
            _ => return Err("Set both PLAINAPP_DATA_DIR and PLAINAPP_CACHE_DIR".into()),
        };
        let private_webview = match private.as_deref().and_then(std::ffi::OsStr::to_str) {
            None if private.is_none() => false,
            Some("0") => false,
            Some("1") => true,
            _ => return Err("PLAINAPP_PRIVATE_WEBVIEW must be 0 or 1".into()),
        };
        Ok(Self {
            directories,
            private_webview,
        })
    }
}

pub(crate) fn private_webview<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    app.try_state::<Options>()
        .is_some_and(|options| options.private_webview)
}

#[cfg(test)]
#[path = "../tests/unit/startup_options.rs"]
mod tests;
