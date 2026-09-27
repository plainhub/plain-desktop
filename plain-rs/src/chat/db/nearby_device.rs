use rusqlite::params;

use super::ChatDb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DNearbyDeviceCache {
    pub id: String,
    pub name: String,
    pub ips: Vec<String>,
    pub port: u16,
    pub device_type: String,
    pub version: String,
    pub platform: String,
    pub last_seen: i64,
}

impl ChatDb {
    pub fn get_cached_nearby_devices(&self) -> rusqlite::Result<Vec<DNearbyDeviceCache>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id,name,ips,port,device_type,version,platform,last_seen \
             FROM nearby_device_cache ORDER BY last_seen DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let ips: String = row.get(2)?;
            Ok(DNearbyDeviceCache {
                id: row.get(0)?,
                name: row.get(1)?,
                ips: ips
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                port: row.get::<_, i64>(3)? as u16,
                device_type: row.get(4)?,
                version: row.get(5)?,
                platform: row.get(6)?,
                last_seen: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn save_cached_nearby_device(&self, device: &DNearbyDeviceCache) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO nearby_device_cache (id,name,ips,port,device_type,version,platform,last_seen)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, ips=excluded.ips, port=excluded.port,
               device_type=excluded.device_type, version=excluded.version,
               platform=excluded.platform,
               last_seen=MAX(nearby_device_cache.last_seen + 1, excluded.last_seen)",
            params![device.id, device.name, device.ips.join(","), device.port as i64,
                device.device_type, device.version, device.platform, device.last_seen],
        )?;
        Ok(())
    }

    pub fn refresh_cached_nearby_device_if_last_seen_matches(
        &self,
        id: &str,
        last_seen: i64,
        now: i64,
    ) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute(
            "UPDATE nearby_device_cache SET last_seen=MAX(last_seen + 1, ?3) WHERE id=?1 AND last_seen=?2",
            params![id, last_seen, now],
        )? == 1)
    }

    pub fn delete_cached_nearby_device_if_last_seen_matches(
        &self,
        id: &str,
        last_seen: i64,
    ) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM nearby_device_cache WHERE id=?1 AND last_seen=?2",
            params![id, last_seen],
        )? == 1)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/db/nearby_device.rs"]
mod tests;
