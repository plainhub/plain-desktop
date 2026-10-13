use plain_rs::chat::enums::DeviceType;
use plain_rs::discovery::advertisement::Facts;

pub fn discovery_facts(device_name: String, app_version: String) -> Result<Facts, String> {
    Ok(Facts {
        name: device_name,
        device_type: DeviceType::Computer,
        version: app_version,
        platform: std::env::consts::OS.to_uppercase(),
        ips: plain_rs::protocols::mdns::host_responder::local_ipv4_strs(),
        aware_supported: false,
        aware_running: false,
    })
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/headless/native/discovery.rs"]
mod tests;
