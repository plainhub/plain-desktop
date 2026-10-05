use super::server::ServerState;
use crate::db::Db;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Archives,
    Archive { id: String, date: String },
    Unarchive { id: String },
    Trashed,
    Trash { ids: Vec<String> },
    Restore { ids: Vec<String> },
}

fn execute(db: &Db, request: Request) -> anyhow::Result<Value> {
    Ok(match request {
        Request::Archives => json!({"items": db.archived_conversation_list()?}),
        Request::Archive { id, date } => {
            if id.is_empty() {
                anyhow::bail!("conversation id is empty");
            }
            let date = chrono::DateTime::parse_from_rfc3339(&date)?.to_rfc3339();
            db.archived_conversation_save(&crate::db::ArchivedConversationRow {
                conversation_id: id,
                conversation_date: date,
            })?;
            json!({"ok": true})
        }
        Request::Unarchive { id } => json!({"count": db.archived_conversation_delete(&id)?}),
        Request::Trashed => json!({"ids": db.trashed_message_ids()?}),
        Request::Trash { ids } => json!({"count": db.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut count = 0;
            {
                let mut statement = tx.prepare("INSERT OR IGNORE INTO trashed_sms (message_id,is_mms,trashed_at) VALUES (?1,?2,?3)")?;
                for id in ids {
                    count += statement.execute(rusqlite::params![id, id.starts_with("mms_"), now])?;
                }
            }
            tx.commit()?;
            Ok::<usize,rusqlite::Error>(count)
        })?}),
        Request::Restore { ids } => json!({"count": db.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut count = 0;
            {
                let mut statement = tx.prepare("DELETE FROM trashed_sms WHERE message_id=?1")?;
                for id in ids { count += statement.execute([id])?; }
            }
            tx.commit()?;
            Ok::<usize,rusqlite::Error>(count)
        })?}),
    })
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state.db, request) {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/sms_state.rs"]
mod tests;
