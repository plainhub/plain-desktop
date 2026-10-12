use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug)]
pub struct NearbyDevice {
    pub id: String,
    pub name: String,
    #[graphql(default)]
    pub ips: Vec<String>,
    pub port: i32,
    pub device_type: String,
    pub version: String,
    pub platform: String,
    pub last_seen: String,
    #[graphql(default)]
    pub discovery_methods: Vec<String>,
    #[graphql(default)]
    pub best_ip: String,
    pub status: String,
}

impl NearbyDevice {
    pub fn from_snapshot_device(device: plain_rs::discovery::nearby_devices::Device) -> Self {
        let interfaces = plain_rs::transport::lan::local_interfaces();
        let best_ip = plain_rs::transport::lan::best(&device.ips, &interfaces);
        Self {
            id: device.id,
            name: device.name,
            ips: device.ips,
            port: device.port as i32,
            device_type: device.device_type,
            version: device.version,
            platform: device.platform,
            last_seen: device.last_seen,
            discovery_methods: device.discovery_methods,
            best_ip,
            status: "UNPAIRED".to_string(),
        }
    }
}
