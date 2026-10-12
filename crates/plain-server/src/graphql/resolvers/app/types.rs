use async_graphql::Enum;

/// Optional device features. A client hides a whole section when its
/// capability is absent, which is why this is a list rather than a set of
/// booleans on the app root.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum Capability {
    MediaTrash,
    MirrorAudio,
    DocPreview,
    ImageSearch,
    MediaScan,
    Sms,
    Calls,
    CallPhone,
    Contacts,
    Packages,
    Notes,
    Feeds,
    ScreenMirror,
    ImageEditor,
    Notifications,
    Clipboard,
    Pomodoro,
    LanShare,
    DiskManager,
}

impl Capability {
    /// Unknown members parse to `None` rather than to a capability the
    /// client would render as a section it cannot drive.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "MEDIA_TRASH" => Self::MediaTrash,
            "MIRROR_AUDIO" => Self::MirrorAudio,
            "DOC_PREVIEW" => Self::DocPreview,
            "IMAGE_SEARCH" => Self::ImageSearch,
            "MEDIA_SCAN" => Self::MediaScan,
            "SMS" => Self::Sms,
            "CALLS" => Self::Calls,
            "CALL_PHONE" => Self::CallPhone,
            "CONTACTS" => Self::Contacts,
            "PACKAGES" => Self::Packages,
            "NOTES" => Self::Notes,
            "FEEDS" => Self::Feeds,
            "SCREEN_MIRROR" => Self::ScreenMirror,
            "IMAGE_EDITOR" => Self::ImageEditor,
            "NOTIFICATIONS" => Self::Notifications,
            "CLIPBOARD" => Self::Clipboard,
            "POMODORO" => Self::Pomodoro,
            "LAN_SHARE" => Self::LanShare,
            "DISK_MANAGER" => Self::DiskManager,
            _ => return None,
        })
    }
}
