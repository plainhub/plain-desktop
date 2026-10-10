pub mod channels;
pub mod messages;
pub mod nearby;
pub mod peers;
use anyhow::{Result, bail};
use serde::Deserialize;
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SaveMode {
    Insert,
    Update,
    Upsert,
}
fn validate(id: &str, created: &str, updated: &str) -> Result<()> {
    if id.trim().is_empty() {
        bail!("record id required");
    }
    chrono::DateTime::parse_from_rfc3339(created)?;
    chrono::DateTime::parse_from_rfc3339(updated)?;
    Ok(())
}
