//! async-graphql schema: output types, QueryRoot, MutationRoot.

use async_graphql::{EmptySubscription, MergedObject, Schema};

use crate::graphql::resolvers::app::capability::CapabilityMutation;
use crate::graphql::resolvers::app::capability::CapabilityQuery;
use crate::graphql::resolvers::app::desktop::AppMutation;
use crate::graphql::resolvers::app::desktop::AppQuery;
use crate::graphql::resolvers::app::logs::AppLogsMutation;
use crate::graphql::resolvers::app::logs::AppLogsQuery;
use crate::graphql::resolvers::audio::desktop::AudioMutation;
use crate::graphql::resolvers::audio::desktop::AudioQuery;
use crate::graphql::resolvers::bookmarks::bookmark::BookmarkMutation;
use crate::graphql::resolvers::bookmarks::bookmark::BookmarkQuery;
use crate::graphql::resolvers::chat::channel::ChatChannelMutation;
use crate::graphql::resolvers::chat::desktop::ChatQuery;
use crate::graphql::resolvers::chat::message::ChatMessageMutation;
use crate::graphql::resolvers::database::desktop::DbMutation;
use crate::graphql::resolvers::database::desktop::DbQuery;
use crate::graphql::resolvers::discovery::desktop::DiscoverMutation;
use crate::graphql::resolvers::discovery::desktop::DiscoverQuery;
use crate::graphql::resolvers::downloads::desktop::DownloadMutation;
use crate::graphql::resolvers::favorites::favorite_folder::FavoriteFolderMutation;
use crate::graphql::resolvers::favorites::favorite_folder::FavoriteFolderQuery;
use crate::graphql::resolvers::feeds::feed::FeedMutation;
use crate::graphql::resolvers::feeds::feed::FeedQuery;
use crate::graphql::resolvers::files::app_file::AppFileQuery;
use crate::graphql::resolvers::files::desktop::FileInfoQuery;
use crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectMutation;
use crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectQuery;
use crate::graphql::resolvers::notes::note::NoteMutation;
use crate::graphql::resolvers::notes::note::NoteQuery;
use crate::graphql::resolvers::peers::desktop::ChatPeerMutation;
use crate::graphql::resolvers::peers::pairing::PairingMutation;
use crate::graphql::resolvers::pomodoro::pomodoro::PomodoroMutation;
use crate::graphql::resolvers::pomodoro::pomodoro::PomodoroQuery;
use crate::graphql::resolvers::settings::desktop::PrefsMutation;
use crate::graphql::resolvers::settings::desktop::PrefsQuery;
use crate::graphql::resolvers::uploads::desktop::FileUploadMutation;
use crate::graphql::resolvers::uploads::desktop::FileUploadQuery;

#[derive(MergedObject, Default)]
pub struct QueryRoot(
    crate::graphql::resolvers::settings::state::PreferencesQuery,
    AppQuery,
    AudioQuery,
    BookmarkQuery,
    CapabilityQuery,
    ChatQuery,
    AppFileQuery,
    AppLogsQuery,
    PrefsQuery,
    DbQuery,
    FileUploadQuery,
    FavoriteFolderQuery,
    ImageEditorProjectQuery,
    FileInfoQuery,
    DiscoverQuery,
    NoteQuery,
    FeedQuery,
    PomodoroQuery,
    crate::graphql::resolvers::media::desktop::MediaQueryRoot,
);

#[derive(MergedObject, Default)]
pub struct MutationRoot(
    crate::graphql::resolvers::settings::state::PreferencesMutation,
    AppMutation,
    AudioMutation,
    BookmarkMutation,
    CapabilityMutation,
    ChatMessageMutation,
    ChatChannelMutation,
    ChatPeerMutation,
    AppLogsMutation,
    PrefsMutation,
    DbMutation,
    FileUploadMutation,
    DiscoverMutation,
    PairingMutation,
    DownloadMutation,
    FavoriteFolderMutation,
    ImageEditorProjectMutation,
    NoteMutation,
    FeedMutation,
    PomodoroMutation,
    crate::graphql::resolvers::media::desktop::MediaMutationRoot,
);

pub type ApiSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema() -> ApiSchema {
    Schema::build(
        QueryRoot::default(),
        MutationRoot::default(),
        EmptySubscription,
    )
    .register_output_type::<crate::graphql::resolvers::media::desktop_item::MediaItem>()
    .finish()
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/schemas/desktop/mod.rs"]
mod tests;
