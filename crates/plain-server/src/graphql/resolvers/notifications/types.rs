use crate::graphql::types::Instant;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: ID,
    /// Android FLAG_ONLY_ALERT_ONCE: re-posted updates must not sound/vibrate again.
    #[graphql(name = "onlyOnce")]
    pub only_once: bool,
    #[graphql(name = "isClearable")]
    pub is_clearable: bool,
    #[graphql(name = "appId")]
    pub app_id: ID,
    #[graphql(name = "appName")]
    pub app_name: String,
    #[graphql(name = "postedAt")]
    pub posted_at: Instant,
    pub silent: bool,
    pub title: String,
    pub body: String,
    pub actions: Vec<String>,
    /// Subset of `actions` that support inline reply; `replyNotification`'s
    /// `actionIndex` indexes this list, not `actions`.
    #[graphql(name = "replyActions")]
    pub reply_actions: Vec<String>,
}
