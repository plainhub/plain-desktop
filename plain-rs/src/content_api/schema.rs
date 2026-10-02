pub use crate::content_types as types;
#[path = "../http_server/main_schemas/content_common.rs"]
mod content_common;
#[path = "../http_server/main_schemas/feed.rs"]
mod feed;
#[path = "../http_server/main_schemas/note.rs"]
mod note;
mod tags;
use async_graphql::{EmptySubscription, MergedObject, Schema};
#[derive(MergedObject, Default)]
pub struct Query(note::NoteQuery, feed::FeedQuery, tags::TagQuery);
#[derive(MergedObject, Default)]
pub struct Mutation(note::NoteMutation, feed::FeedMutation, tags::TagMutation);
pub type ContentSchema = Schema<Query, Mutation, EmptySubscription>;
pub fn build(
    db: std::sync::Arc<crate::db::Db>,
    events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
    prefs: std::sync::Arc<crate::prefs::Prefs>,
    directory: std::path::PathBuf,
) -> ContentSchema {
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(db.clone())
        .data(events.clone())
        .data(prefs)
        .data(crate::feeds::SyncService::new(
            db.clone(),
            events,
            Some(std::sync::Arc::new(crate::feeds::FeedAssets {
                db: db.clone(),
                directory: directory.clone(),
            })),
        ))
        .data(std::sync::Arc::new(crate::feeds::FeedAssets {
            db,
            directory,
        }))
        .finish()
}
