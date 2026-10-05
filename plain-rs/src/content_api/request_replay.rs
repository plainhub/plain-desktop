use std::{collections::HashMap, sync::Mutex};
#[derive(Default)]
pub(super) struct Replay {
    entries: Mutex<HashMap<(String, i64, String), i64>>,
}
impl Replay {
    pub(super) fn admit<'a>(&self, client: &str, plain: &'a str, now: i64) -> Result<&'a str, ()> {
        let mut parts = plain.splitn(3, '|');
        let timestamp = parts.next().ok_or(())?.parse::<i64>().map_err(|_| ())?;
        let nonce = parts.next().ok_or(())?;
        let body = parts.next().ok_or(())?;
        if (i128::from(now) - i128::from(timestamp)).abs() > 60_000 {
            return Err(());
        }
        let key = (client.to_owned(), timestamp, nonce.to_owned());
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, stamp| i128::from(now) - i128::from(*stamp) <= 120_000);
        if entries.contains_key(&key) {
            return Err(());
        }
        entries.insert(key, now);
        Ok(body)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/request_replay.rs"]
mod tests;
