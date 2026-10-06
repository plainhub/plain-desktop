//! The public `/graphql` schema — the contract LAN clients (the web console)
//! query.
//!
//! Deliberately separate from [`super::schema::ContentSchema`]: that one is
//! the app's own host-facing API and carries internal roots
//! (`audioHostQueueItems`, `fileHostTasks`, …) that must never reach the
//! network. Here only the contract's root fields exist, so anything missing
//! from [`crate::content_types`] is a gap to fill rather than a leak.

use super::public_calls::{CallsMutation, CallsQuery};
use super::public_clipboard::{ClipboardMutation, ClipboardQuery};
use super::public_contacts::{ContactsMutation, ContactsQuery};
use super::public_files::{FilesQuery, FavoritesMutation};
use super::public_image_index::{ImageIndexMutation, ImageIndexQuery};
use super::public_media::MediaQuery;
use super::public_tags::{TagsMutation, TagsQuery};
use super::public_notifications::{NotificationsMutation, NotificationsQuery};
use super::public_packages::{PackagesMutation, PackagesQuery};
use super::public_screen_mirror::{ScreenMirrorMutation, ScreenMirrorQuery, SettingsMutation};
use super::public_sms::{SmsMutation, SmsQuery};
use crate::content_api::host::Host;
use crate::{db::Db, prefs::Prefs};
use async_graphql::{EmptySubscription, MergedObject, Schema};
use std::sync::Arc;

#[derive(MergedObject, Default)]
pub struct Query(
    PackagesQuery,
    NotificationsQuery,
    ClipboardQuery,
    ContactsQuery,
    SmsQuery,
    CallsQuery,
    ScreenMirrorQuery,
    ImageIndexQuery,
    FilesQuery,
    MediaQuery,
    TagsQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(
    PackagesMutation,
    NotificationsMutation,
    ClipboardMutation,
    ContactsMutation,
    SmsMutation,
    CallsMutation,
    ScreenMirrorMutation,
    ImageIndexMutation,
    SettingsMutation,
    FavoritesMutation,
    TagsMutation,
);

pub type PublicSchema = Schema<Query, Mutation, EmptySubscription>;

pub fn build(host: Arc<Host>, prefs: Arc<Prefs>, db: Arc<Db>) -> PublicSchema {
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(host)
        .data(prefs)
        .data(db)
        .finish()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_schema.rs"]
mod tests;
