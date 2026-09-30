#[cfg(any(feature = "api", feature = "media_gql"))]
pub mod enums;
#[cfg(feature = "chat")]
pub mod chat;
pub mod crypto;
#[cfg(feature = "library")]
pub mod library;
#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "api")]
pub mod httpserver;
#[cfg(feature = "nas")]
pub mod nas;
#[cfg(feature = "media")]
pub mod media;
pub mod mdns;
#[cfg(feature = "prefs")]
pub mod prefs;
#[cfg(feature = "sqlite_browse")]
pub mod sqlite_browse;
pub mod tls;
pub mod utils;
pub mod ws_frame;

pub use crypto::*;
pub use utils::*;

#[cfg(all(test, feature = "nas"))]
pub(crate) mod test_support {
    //! Shared fixture for the nas schema tests: a real nas-flavored
    //! `ChatState` (chat.db + prefs) over a temp data dir.
    pub(crate) fn chat_state(
        data_dir: &std::path::Path,
    ) -> std::sync::Arc<crate::api::chat::ChatState> {
        let prefs =
            crate::prefs::Prefs::load(&crate::prefs::default_path(data_dir)).expect("prefs load");
        std::sync::Arc::new(
            crate::api::chat::ChatState::nas_init(data_dir, &prefs).expect("chat init"),
        )
    }

    pub(crate) fn library(
        data_dir: &std::path::Path,
    ) -> std::sync::Arc<crate::library::db::LibraryDb> {
        std::sync::Arc::new(
            crate::library::db::LibraryDb::open(&data_dir.join("library.db")).expect("open library.db"),
        )
    }
}
