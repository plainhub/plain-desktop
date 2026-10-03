/// Audio playback repeat mode, mirroring the plain-app `MediaPlayMode` enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(
    any(feature = "api", feature = "media_gql", feature = "content_api"),
    derive(async_graphql::Enum)
)]
#[cfg_attr(
    any(feature = "api", feature = "media_gql", feature = "content_api"),
    graphql(rename_items = "SCREAMING_SNAKE_CASE")
)]
pub enum MediaPlayMode {
    Repeat,
    RepeatOne,
    Shuffle,
}
impl MediaPlayMode {
    pub fn ordinal(self) -> i32 {
        match self {
            Self::Repeat => 0,
            Self::RepeatOne => 1,
            Self::Shuffle => 2,
        }
    }
    pub fn from_ordinal(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Repeat),
            1 => Some(Self::RepeatOne),
            2 => Some(Self::Shuffle),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Repeat => "REPEAT",
            Self::RepeatOne => "REPEAT_ONE",
            Self::Shuffle => "SHUFFLE",
        }
    }
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "REPEAT" => Some(Self::Repeat),
            "REPEAT_ONE" => Some(Self::RepeatOne),
            "SHUFFLE" => Some(Self::Shuffle),
            _ => None,
        }
    }
}
