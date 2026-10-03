use super::validate;
use crate::db::{DNearbyDeviceCache, Db, now_iso};
use anyhow::Result;
use rusqlite::params;
pub fn all(db: &Db) -> Result<Vec<DNearbyDeviceCache>> {
    Ok(db.with_conn(|c|{
 let mut s=c.prepare("SELECT id,name,ips,port,device_type,version,platform,last_seen FROM nearby_device_cache ORDER BY julianday(last_seen) DESC,id")?;
 s.query_map([],|r|Ok(DNearbyDeviceCache {id:r.get(0)?,name:r.get(1)?,ips:r.get::<_,String>(2)?.split(',').map(str::trim).filter(|v|!v.is_empty()).map(str::to_owned).collect(),port:r.get(3)?,device_type:r.get(4)?,version:r.get(5)?,platform:r.get(6)?,last_seen:r.get(7)?}))?.collect::<rusqlite::Result<Vec<_>>>()
 })?)
}
pub fn save(db: &Db, row: &DNearbyDeviceCache) -> Result<()> {
    validate(&row.id, &row.last_seen, &row.last_seen)?;
    db.with_conn(|c|c.execute("INSERT INTO nearby_device_cache(id,name,ips,port,device_type,version,platform,last_seen) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET name=excluded.name,ips=excluded.ips,port=excluded.port,device_type=excluded.device_type,version=excluded.version,platform=excluded.platform,last_seen=excluded.last_seen WHERE julianday(excluded.last_seen)>=julianday(nearby_device_cache.last_seen)",params![row.id,row.name,row.ips.join(","),row.port,row.device_type,row.version,row.platform,row.last_seen]))?;
    Ok(())
}
pub fn touch(db: &Db, id: &str) -> Result<bool> {
    Ok(db.with_conn(|c| {
        c.execute(
            "UPDATE nearby_device_cache SET last_seen=?2 WHERE id=?1",
            params![id, now_iso()],
        )
    })? == 1)
}
pub fn delete(db: &Db, id: &str) -> Result<bool> {
    Ok(db.with_conn(|c| c.execute("DELETE FROM nearby_device_cache WHERE id=?1", [id]))? == 1)
}
