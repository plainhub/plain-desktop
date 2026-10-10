use crate::graphql::resolvers::audio::desktop_types::Audio;
use crate::graphql::resolvers::docs::types::Doc;
use crate::graphql::resolvers::images::types::Image;
use crate::graphql::resolvers::videos::types::Video;
use async_graphql::Enum;
use async_graphql::ID;
use async_graphql::SimpleObject;
use plain_rs::enums::DataType;

/// Media-library item kinds accepted by media item actions and buckets
/// (plain-app `MediaDataType`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum MediaDataType {
    AUDIO,
    VIDEO,
    IMAGE,
    DOC,
}

impl MediaDataType {
    pub fn data_type(self) -> DataType {
        match self {
            MediaDataType::DOC => DataType::Doc,
            MediaDataType::AUDIO => DataType::Audio,
            MediaDataType::VIDEO => DataType::Video,
            MediaDataType::IMAGE => DataType::Image,
        }
    }
}

/// Media-index scan lifecycle, shared by `scanProgress.state` and the
/// `media:scan:progress` WS payload (same values on both wires).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum ScanState {
    IDLE,
    RUNNING,
    PAUSED,
    STOPPED,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ScanProgress {
    pub indexed: i32,
    pub pending: i32,
    pub total: i32,
    pub state: ScanState,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct MediaBucket {
    pub id: ID,
    pub name: String,
    #[graphql(name = "itemCount")]
    pub item_count: i32,
    #[graphql(name = "topItemPaths")]
    pub top_item_paths: Vec<String>,
}
