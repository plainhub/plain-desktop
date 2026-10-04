use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub ips: Vec<String>,
    pub port: u16,
    pub device_type: String,
    pub version: String,
    pub platform: String,
    pub last_seen: String,
    #[serde(default)]
    pub discovery_methods: Vec<String>,
}
impl Device {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && self.id.len() <= 256,
            "Invalid nearby identity"
        );
        ensure!(
            self.ips.len() <= 64
                && !self.discovery_methods.is_empty()
                && self
                    .discovery_methods
                    .iter()
                    .all(|v| v == "LAN" || v == "BLE"),
            "Invalid nearby facts"
        );
        ensure!(
            chrono::DateTime::parse_from_rfc3339(&self.last_seen).is_ok(),
            "Invalid nearby time"
        );
        serde_json::from_value::<super::enums::DeviceType>(serde_json::json!(self.device_type))?;
        Ok(())
    }
    pub fn cache(&self) -> crate::db::DNearbyDeviceCache {
        crate::db::DNearbyDeviceCache {
            id: self.id.clone(),
            name: self.name.clone(),
            ips: self.ips.clone(),
            port: self.port,
            device_type: self.device_type.clone(),
            version: self.version.clone(),
            platform: self.platform.clone(),
            last_seen: self.last_seen.clone(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub devices: Vec<Device>,
}
#[derive(Clone)]
pub struct Probe {
    pub device: Device,
    generation: String,
    cache_seen: Option<String>,
}
struct Entry {
    device: Device,
    generation: String,
    seen: Instant,
    age: Duration,
    emitted: Option<Instant>,
}
#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    order: Vec<String>,
    revision: u64,
    lan: bool,
    ble: bool,
}
#[derive(Default)]
pub struct Devices(Mutex<State>);
impl Devices {
    pub fn forget(&self, id: &str) -> bool {
        let mut s = self.0.lock().unwrap();
        if s.entries.remove(id).is_none() {
            return false;
        }
        s.order.retain(|v| v != id);
        s.revision += 1;
        true
    }
    pub fn active(&self) -> bool {
        let s = self.0.lock().unwrap();
        s.lan || s.ble
    }
    pub fn scanning(
        &self,
        lan: bool,
        ble: bool,
        cached: Vec<crate::db::DNearbyDeviceCache>,
    ) -> Result<()> {
        self.scanning_modes(Some(lan), Some(ble), cached)
    }
    pub fn scanning_modes(
        &self,
        lan: Option<bool>,
        ble: Option<bool>,
        cached: Vec<crate::db::DNearbyDeviceCache>,
    ) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        let lan = lan.unwrap_or(s.lan);
        let ble = ble.unwrap_or(s.ble);
        if s.lan != lan || s.ble != ble {
            for entry in s.entries.values_mut() {
                entry.generation = uuid::Uuid::new_v4().to_string();
            }
        }
        s.lan = lan;
        s.ble = ble;
        if s.entries.is_empty() && (lan || ble) {
            let now = Instant::now();
            for d in cached.into_iter().take(512) {
                let age = chrono::DateTime::parse_from_rfc3339(&d.last_seen)
                    .map(|t| {
                        (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                            .num_seconds()
                            .max(0) as u64
                    })
                    .unwrap_or(61)
                    .min(86400 * 365);
                let device = Device {
                    id: d.id,
                    name: d.name,
                    ips: d.ips,
                    port: d.port,
                    device_type: d.device_type,
                    version: d.version,
                    platform: d.platform,
                    last_seen: d.last_seen,
                    discovery_methods: vec!["LAN".into()],
                };
                s.order.push(device.id.clone());
                s.entries.insert(
                    device.id.clone(),
                    Entry {
                        device,
                        generation: uuid::Uuid::new_v4().to_string(),
                        seen: now,
                        age: Duration::from_secs(age),
                        emitted: None,
                    },
                );
            }
            s.revision += 1;
        }
        Ok(())
    }
    pub fn observe(&self, device: Device, visible: bool) -> Result<bool> {
        self.observe_at(device, Instant::now(), visible)
    }
    #[cfg(test)]
    fn seen_at(&self, device: Device, now: Instant) -> Result<bool> {
        self.observe_at(device, now, true)
    }
    fn observe_at(&self, mut device: Device, now: Instant, visible: bool) -> Result<bool> {
        device.validate()?;
        let mut s = self.0.lock().unwrap();
        let emitted = s.entries.get(&device.id).and_then(|e| e.emitted);
        let emit = visible
            && emitted.is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(1));
        if let Some(old) = s.entries.get(&device.id) {
            let mut ips = old.device.ips.clone();
            for ip in &device.ips {
                if !ips.contains(ip) {
                    ips.push(ip.clone());
                }
            }
            ips.truncate(64);
            device.ips = ips;
            let mut methods = old.device.discovery_methods.clone();
            for method in &device.discovery_methods {
                if !methods.contains(method) {
                    methods.push(method.clone());
                }
            }
            device.discovery_methods = methods;
            if chrono::DateTime::parse_from_rfc3339(&old.device.last_seen)?
                > chrono::DateTime::parse_from_rfc3339(&device.last_seen)?
            {
                device.last_seen = old.device.last_seen.clone();
            }
        } else {
            if s.entries.len() >= 512 {
                let id = s
                    .entries
                    .iter()
                    .min_by_key(|(_, e)| e.seen)
                    .map(|(id, _)| id.clone())
                    .unwrap();
                s.entries.remove(&id);
                s.order.retain(|v| v != &id);
            }
            s.order.push(device.id.clone());
        }
        s.entries.insert(
            device.id.clone(),
            Entry {
                device,
                generation: uuid::Uuid::new_v4().to_string(),
                seen: now,
                age: Duration::ZERO,
                emitted: if emit { Some(now) } else { emitted },
            },
        );
        s.revision += 1;
        Ok(emit)
    }
    pub fn snapshot(&self) -> Snapshot {
        let s = self.0.lock().unwrap();
        Snapshot {
            revision: s.revision,
            devices: s
                .order
                .iter()
                .filter_map(|id| s.entries.get(id).map(|e| e.device.clone()))
                .collect(),
        }
    }
    pub fn stale(&self, db: &crate::db::Db) -> Result<Vec<Probe>> {
        self.stale_at(db, Instant::now())
    }
    fn stale_at(&self, db: &crate::db::Db, now: Instant) -> Result<Vec<Probe>> {
        let s = self.0.lock().unwrap();
        if !(s.lan || s.ble) {
            return Ok(vec![]);
        }
        let cached: HashMap<_, _> = crate::db::chat_store::nearby::all(db)?
            .into_iter()
            .map(|d| (d.id, d.last_seen))
            .collect();
        Ok(s.entries
            .values()
            .filter(|e| {
                now.saturating_duration_since(e.seen).saturating_add(e.age)
                    > Duration::from_secs(60)
            })
            .map(|e| Probe {
                device: e.device.clone(),
                generation: e.generation.clone(),
                cache_seen: cached.get(&e.device.id).cloned(),
            })
            .collect())
    }
    pub fn verified(&self, db: &crate::db::Db, probe: &Probe, alive: bool) -> Result<bool> {
        let mut s = self.0.lock().unwrap();
        if !(s.lan || s.ble)
            || !s
                .entries
                .get(&probe.device.id)
                .is_some_and(|e| e.generation == probe.generation)
        {
            return Ok(false);
        }
        let cached = crate::db::chat_store::nearby::all(db)?
            .into_iter()
            .find(|d| d.id == probe.device.id);
        if cached.as_ref().map(|d| &d.last_seen) != probe.cache_seen.as_ref() {
            return Ok(false);
        }
        if alive {
            let now = crate::db::now_iso();
            let updated = db.refresh_cached_nearby_device_if_last_seen_matches(
                &probe.device.id,
                probe.cache_seen.as_deref().unwrap_or(""),
                &now,
            )?;
            if cached.is_some() && !updated {
                return Ok(false);
            }
            let e = s.entries.get_mut(&probe.device.id).unwrap();
            e.seen = Instant::now();
            e.age = Duration::ZERO;
            e.device.last_seen = now;
            e.generation = uuid::Uuid::new_v4().to_string();
        } else {
            let updated = db.delete_cached_nearby_device_if_last_seen_matches(
                &probe.device.id,
                probe.cache_seen.as_deref().unwrap_or(""),
            )?;
            if cached.is_some() && !updated {
                return Ok(false);
            }
            s.entries.remove(&probe.device.id);
            s.order.retain(|v| v != &probe.device.id);
        }
        s.revision += 1;
        Ok(true)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/chat/nearby_devices.rs"]
mod tests;
