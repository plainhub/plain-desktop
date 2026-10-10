//! Wire enums shared by every GraphQL host — the desktop `api` stack and
//! the `media_gql` roots — mirroring plain-app's shared `enums` package.
//! One copy per wire name; hosts must never define a second one or the
//! merged schema panics at build.

/// Taggable data domains — the plain-app `DataType` wire enum
/// (`shared/src/commonMain/kotlin/com/ismartcoding/plain/enums/DataType.kt`).
/// GraphQL-arg and tag-column domain; `kind()` is the ordinal stored in
/// the shared library db `type` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
#[cfg_attr(
    feature = "graphql",
    graphql(name = "DataType", rename_items = "SCREAMING_SNAKE_CASE")
)]
pub enum DataType {
    Default,
    Audio,
    Video,
    Image,
    Sms,
    Contact,
    Note,
    FeedEntry,
    Call,
    Package,
    File,
    AppFile,
    Doc,
}

impl DataType {
    /// Numeric plain-app `DataType` ordinal — the tag's stored `type`
    /// column.
    pub fn kind(self) -> i32 {
        match self {
            Self::Default => 0,
            Self::Audio => 1,
            Self::Video => 2,
            Self::Image => 3,
            Self::Sms => 4,
            Self::Contact => 5,
            Self::Note => 6,
            Self::FeedEntry => 7,
            Self::Call => 8,
            Self::Package => 21,
            Self::File => 22,
            Self::AppFile => 23,
            Self::Doc => 24,
        }
    }

    /// Media index type filter — "audio" / "video" / "image" / "doc";
    /// `None` for domains the media index does not cover.
    pub fn media_type_str(self) -> Option<&'static str> {
        match self {
            Self::Audio => Some("audio"),
            Self::Video => Some("video"),
            Self::Image => Some("image"),
            Self::Doc => Some("doc"),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/enums.rs"]
mod tests;
