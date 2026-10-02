#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DNearbyDeviceCache {
    pub id: String,
    pub name: String,
    pub ips: Vec<String>,
    pub port: u16,
    pub device_type: String,
    pub version: String,
    pub platform: String,
    pub last_seen: String,
}
