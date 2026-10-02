/// Rows for the fixed-query tables that have no search DSL: clipboards,
/// sessions, shares, pomodoro items, media duration cache, video play
/// progress, image embeddings, archived conversations and trashed SMS.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClipboardRow {
    pub id: String,
    pub text: String,
    pub hash: String,
    pub source: String,
    pub label: String,
    pub sensitive: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionRow {
    pub client_id: String,
    pub name: String,
    pub r#type: String,
    pub client_ip: String,
    pub os_name: String,
    pub os_version: String,
    pub browser_name: String,
    pub browser_version: String,
    pub token: String,
    pub last_active_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ShareRow {
    pub id: String,
    pub name: String,
    pub password: String,
    pub url_token: String,
    pub expires_at: Option<String>,
    pub read_only: bool,
    pub data: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PomodoroItemRow {
    pub id: String,
    pub date: String,
    pub completed_count: i32,
    pub total_work_seconds: i32,
    pub total_break_seconds: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MediaItemRow {
    pub media_type: String,
    pub media_id: String,
    pub duration_ms: i64,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct VideoPlayProgressRow {
    pub media_id: String,
    pub position_ms: i64,
    pub updated_at: String,
}

/// The embedding blob is base64-encoded in JSON so the FFI payload stays
/// text-safe across the JNI and C ABI boundaries.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ImageEmbeddingRow {
    pub id: String,
    pub path: String,
    pub embedding_base64: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ArchivedConversationRow {
    pub conversation_id: String,
    pub conversation_date: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TrashedMessageRow {
    pub message_id: String,
    pub is_mms: bool,
    pub trashed_at: String,
}
