use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateInfo {
    pub new_version: String,
    pub check_update_time: u64,
    pub skip_version: String,
    pub publish_date: String,
    pub log: String,
    pub download_url: String,
    pub size: u64,
    pub downloaded_apk_path: String,
    pub auto_check_update: bool,
}

impl Default for UpdateInfo {
    fn default() -> Self {
        Self {
            new_version: String::new(),
            check_update_time: 0,
            skip_version: String::new(),
            publish_date: String::new(),
            log: String::new(),
            download_url: String::new(),
            size: 0,
            downloaded_apk_path: String::new(),
            auto_check_update: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePatch {
    pub new_version: Option<String>,
    pub check_update_time: Option<u64>,
    pub skip_version: Option<String>,
    pub publish_date: Option<String>,
    pub log: Option<String>,
    pub download_url: Option<String>,
    pub size: Option<u64>,
    pub downloaded_apk_path: Option<String>,
    pub auto_check_update: Option<bool>,
}

impl UpdatePatch {
    pub(super) fn apply(self, info: &mut UpdateInfo) {
        macro_rules! patch { ($($field:ident),*) => { $(if let Some(value) = self.$field { info.$field = value; })* }; }
        patch!(
            new_version,
            check_update_time,
            skip_version,
            publish_date,
            log,
            download_url,
            size,
            downloaded_apk_path,
            auto_check_update
        );
    }
}
