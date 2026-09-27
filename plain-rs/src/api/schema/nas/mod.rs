//! GraphQL schema. The Go side uses `gqlgen` to generate code from
//! `internal/graph/schema.graphql`. Here we define the same shape with
//! `async-graphql` macros. We deliberately do not embed the full
//! `schema.graphql` SDL file - the Rust types are the source of truth, so
//! the SDL exposed to clients is rebuilt from them.

pub mod mutation;
pub mod peer_schema;
pub mod query;
pub mod types;

use async_graphql::{EmptySubscription, MergedObject, Schema};

/// The NAS-specific Query fields + the shared media/file/tag surface from
/// plain-rs. Field sets union, so the SDL stays the same shape as before
/// the media roots moved into plain-rs.
#[derive(MergedObject)]
pub struct QueryRoot(query::NasQueryRoot, plain_rs::media::gql::MediaQueryRoot);

#[derive(MergedObject)]
pub struct MutationRoot(mutation::NasMutationRoot, plain_rs::media::gql::MediaMutationRoot);

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

/// Run blocking work (file probes, fjall batches) off the async runtime.
/// Metadata hydration parses files — a lofty read can touch a whole MP3 —
/// and must never occupy a tokio worker thread.
pub(crate) async fn run_blocking<T, F>(f: F) -> async_graphql::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| async_graphql::Error::new(format!("blocking task failed: {e}")))
}

/// Build a schema once at startup. Registers `Arc<Db>`, `Arc<Prefs>`,
/// `Arc<Config>` and the data dir as **global** context data so every
/// resolver can access them via `ctx.data::<…>()`. The user library
/// (`library.db` — audio queue/playlists/history, tags, favorite folders)
/// opens here under the data dir and registers the same way.
pub fn build_schema(
    db: std::sync::Arc<crate::db::Db>,
    prefs: std::sync::Arc<crate::prefs::Prefs>,
    config: std::sync::Arc<crate::config::Config>,
    data_dir: std::path::PathBuf,
    chat: std::sync::Arc<crate::chat::ChatState>,
) -> AppSchema {
    let library = std::sync::Arc::new(
        plain_rs::api::server::nas_ctx::open_library_db(&data_dir)
            .expect("open library.db under data dir"),
    );
    Schema::build(
        QueryRoot(
            query::NasQueryRoot::new(db.clone(), prefs.clone(), config.clone(), data_dir.clone()),
            plain_rs::media::gql::MediaQueryRoot,
        ),
        MutationRoot(mutation::NasMutationRoot, plain_rs::media::gql::MediaMutationRoot),
        EmptySubscription,
    )
    .data(db)
    .data(prefs)
    .data(config)
    .data(data_dir)
    .data(chat)
    .data(library)
    .finish()
}

/// This-phase shim over [`AppSchema`] implementing plain-rs's
/// type-erased `GraphqlExec`: injects every resolver-data Arc (the
/// single-process fjall/prefs/config/chat/library handles) plus the
/// requesting cid into each `Request` before executing. Dies in phase 3
/// when the nas schema folds into the plain-rs one.
pub struct NasSchemaExec {
    pub schema: AppSchema,
    pub db: std::sync::Arc<crate::db::Db>,
    pub prefs: std::sync::Arc<crate::prefs::Prefs>,
    pub config: std::sync::Arc<crate::config::Config>,
    pub data_dir: std::path::PathBuf,
    pub chat: std::sync::Arc<crate::chat::ChatState>,
    pub library: std::sync::Arc<plain_rs::library::db::LibraryDb>,
}

impl plain_rs::api::server::GraphqlExec for NasSchemaExec {
    fn execute(
        &self,
        mut request: async_graphql::Request,
        cid: &str,
    ) -> futures::future::BoxFuture<'_, async_graphql::Response> {
        request = request
            .data(self.db.clone())
            .data(self.prefs.clone())
            .data(self.config.clone())
            .data(self.data_dir.clone())
            .data(self.chat.clone())
            .data(self.library.clone())
            .data(cid.to_string());
        let schema = self.schema.clone();
        Box::pin(async move { schema.execute(request).await })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// This-phase shim over the nas [`peer_schema::PeerSchema`]
/// implementing plain-rs's type-erased `PeerSchemaExec` (dies with
/// `NasSchemaExec` in phase 3).
pub struct NasPeerSchemaExec(pub peer_schema::PeerSchema);

impl plain_rs::api::server::PeerSchemaExec for NasPeerSchemaExec {
    fn execute(
        &self,
        request: async_graphql::Request,
        peer: plain_rs::chat::db::DPeer,
        channel_id: &str,
        chat: std::sync::Arc<crate::chat::ChatState>,
    ) -> futures::future::BoxFuture<'_, async_graphql::Response> {
        let peer_ctx = peer_schema::PeerCtx {
            state: chat,
            peer,
            channel_id: channel_id.to_string(),
        };
        let schema = self.0.clone();
        Box::pin(async move { schema.execute(request.data(peer_ctx)).await })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/gql/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/gql/router.rs"]
mod router_tests;
