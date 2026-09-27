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
        crate::library::open(&data_dir).expect("open library.db under data dir"),
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

#[cfg(test)]
#[path = "../../tests/unit/gql/mod.rs"]
mod tests;
