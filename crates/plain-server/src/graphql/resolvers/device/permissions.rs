use async_graphql::Enum;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum Permission {
    WriteExternalStorage,
    ReadSms,
    SendSms,
    ReadContacts,
    WriteContacts,
    ReadCallLog,
    WriteCallLog,
    CallPhone,
    PostNotifications,
    NearbyWifiDevices,
    AccessFineLocation,
    Camera,
    SystemAlertWindow,
    RecordAudio,
    ReadMediaImages,
    ReadMediaVideos,
    ReadMediaAudio,
    NotificationListener,
    ReadPhoneState,
    ReadPhoneNumbers,
    ScheduleExactAlarm,
    QueryAllPackages,
    Adb,
    Clipboard,
}

impl Permission {
    /// Unknown members parse to `None`: a permission this build has never
    /// heard of is not one the client can request.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "WRITE_EXTERNAL_STORAGE" => Self::WriteExternalStorage,
            "READ_SMS" => Self::ReadSms,
            "SEND_SMS" => Self::SendSms,
            "READ_CONTACTS" => Self::ReadContacts,
            "WRITE_CONTACTS" => Self::WriteContacts,
            "READ_CALL_LOG" => Self::ReadCallLog,
            "WRITE_CALL_LOG" => Self::WriteCallLog,
            "CALL_PHONE" => Self::CallPhone,
            "POST_NOTIFICATIONS" => Self::PostNotifications,
            "NEARBY_WIFI_DEVICES" => Self::NearbyWifiDevices,
            "ACCESS_FINE_LOCATION" => Self::AccessFineLocation,
            "CAMERA" => Self::Camera,
            "SYSTEM_ALERT_WINDOW" => Self::SystemAlertWindow,
            "RECORD_AUDIO" => Self::RecordAudio,
            "READ_MEDIA_IMAGES" => Self::ReadMediaImages,
            "READ_MEDIA_VIDEOS" => Self::ReadMediaVideos,
            "READ_MEDIA_AUDIO" => Self::ReadMediaAudio,
            "NOTIFICATION_LISTENER" => Self::NotificationListener,
            "READ_PHONE_STATE" => Self::ReadPhoneState,
            "READ_PHONE_NUMBERS" => Self::ReadPhoneNumbers,
            "SCHEDULE_EXACT_ALARM" => Self::ScheduleExactAlarm,
            "QUERY_ALL_PACKAGES" => Self::QueryAllPackages,
            "ADB" => Self::Adb,
            "CLIPBOARD" => Self::Clipboard,
            _ => return None,
        })
    }
}
