use plain_rs::events::WsEvent;
use plain_rs::prefs::Prefs;
use serde_json::{Value, json};
use tokio::sync::broadcast;

pub(crate) fn set(
    prefs: &Prefs,
    events: &broadcast::Sender<WsEvent>,
    user: bool,
    key: &str,
    value: Value,
) -> Result<bool, String> {
    let changed = if user {
        if let Some(changed) = plain_rs::prefs::user::set_entry(prefs, key, value.clone())
            .map_err(|e| e.to_string())?
        {
            changed
        } else {
            prefs
                .set_user(key, &value)
                .map_err(|error| error.to_string())?
        }
    } else {
        plain_rs::prefs::system::set_entry(prefs, key, value.clone())
            .map_err(|error| error.to_string())?
    };
    if changed {
        let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
        if user {
            let event = match key {
                plain_rs::prefs::user::keys::DEVICE_NAME => {
                    Some(("DEVICE_NAME_UPDATED", value.to_string()))
                }
                plain_rs::prefs::user::keys::NOTIFICATION_FILTER => {
                    Some(("NOTIFICATION_REFRESHED", String::new()))
                }
                plain_rs::prefs::user::keys::POMODORO_SETTINGS => {
                    Some(("POMODORO_SETTINGS_UPDATE", value.to_string()))
                }
                _ => None,
            };
            if let Some((kind, payload)) = event {
                let _ = events.send(WsEvent::broadcast(kind, payload));
            }
        }
    }
    Ok(changed)
}

pub(crate) fn remove(
    prefs: &Prefs,
    events: &broadcast::Sender<WsEvent>,
    user: bool,
    key: &str,
) -> Result<bool, String> {
    let changed = if user {
        prefs.remove_user(key)
    } else {
        plain_rs::prefs::system::remove_entry(prefs, key)
    }
    .map_err(|e| e.to_string())?;
    if changed {
        let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
    }
    Ok(changed)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/settings/preferences.rs"]
mod tests;
