use crate::graphql::types::ActionResult;
use crate::graphql::types::Instant;
use async_graphql::{Context, ID, InputObject, Object, Result, SimpleObject};
use plain_rs::db::ClipboardRow;
use plain_rs::db::Db;
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct ClipboardItem {
    id: ID,
    text: String,
    /// Client id of the sync origin: empty = captured locally on this device, otherwise received from that peer.
    source: String,
    label: String,
    sensitive: bool,
    created_at: Instant,
}
impl TryFrom<ClipboardRow> for ClipboardItem {
    type Error = async_graphql::Error;
    fn try_from(row: ClipboardRow) -> Result<Self> {
        Ok(Self {
            id: ID(row.id),
            text: row.text,
            source: row.source,
            label: row.label,
            sensitive: row.sensitive,
            created_at: Instant(
                chrono::DateTime::parse_from_rfc3339(&row.created_at)?.with_timezone(&chrono::Utc),
            ),
        })
    }
}

#[derive(InputObject)]
pub struct ClipboardRecordInput {
    text: String,
    source: String,
    label: String,
    sensitive: bool,
}

#[derive(SimpleObject)]
pub struct ClipboardRecordResult {
    inserted: bool,
    item: ClipboardItem,
}

#[derive(Default)]
pub struct ClipboardHostMutation;
#[Object]
impl ClipboardHostMutation {
    async fn record_clipboard(
        &self,
        ctx: &Context<'_>,
        input: ClipboardRecordInput,
    ) -> Result<ClipboardRecordResult> {
        let (item, inserted) = plain_rs::clipboard::record(
            ctx.data::<Arc<Db>>()?,
            &input.text,
            &input.source,
            &input.label,
            input.sensitive,
        )
        .map_err(async_graphql::Error::new)?;
        Ok(ClipboardRecordResult {
            inserted,
            item: item.try_into()?,
        })
    }
    async fn delete_clipboard_items_by_ids(
        &self,
        ctx: &Context<'_>,
        ids: Vec<ID>,
    ) -> Result<ActionResult> {
        Ok(ActionResult {
            affected_count: ctx
                .data::<Arc<Db>>()?
                .clipboard_delete_by_ids(&ids.into_iter().map(|id| id.0).collect::<Vec<_>>())?
                .try_into()?,
        })
    }
}
