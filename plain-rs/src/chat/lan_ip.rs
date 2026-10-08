use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Interface {
    pub ip: Ipv4Addr,
    pub prefix_length: u8,
}
pub fn local_interfaces() -> Vec<Interface> {
    crate::utils::ifaddr::list()
        .into_iter()
        .filter_map(|interface| {
            if interface.ip.is_unspecified() {
                return None;
            }
            // A mask that is not a run of leading ones means the kernel gave
            // us something we cannot read as a prefix length.
            let mask = u32::from(interface.netmask?);
            let prefix_length = mask.leading_ones() as u8;
            let expected = if prefix_length == 0 {
                0
            } else {
                u32::MAX << (32 - prefix_length)
            };
            (mask == expected).then_some(Interface {
                ip: interface.ip,
                prefix_length,
            })
        })
        .collect()
}
pub fn best(ips: &[String], local: &[Interface]) -> String {
    let valid: Vec<_> = ips
        .iter()
        .filter_map(|ip| ip.parse::<IpAddr>().ok().map(|parsed| (ip, parsed)))
        .collect();
    for (ip, parsed) in &valid {
        if let IpAddr::V4(address) = parsed {
            if local.iter().any(|interface| {
                if interface.prefix_length > 32 {
                    return false;
                }
                let mask = if interface.prefix_length == 0 {
                    0
                } else {
                    u32::MAX << (32 - interface.prefix_length)
                };
                u32::from(*address) & mask == u32::from(interface.ip) & mask
            }) {
                return (*ip).clone();
            }
        }
    }
    valid
        .iter()
        .find(|(_, parsed)| matches!(parsed,IpAddr::V4(address) if address.is_private()))
        .or(valid.first())
        .map(|(ip, _)| (*ip).clone())
        .unwrap_or_default()
}
#[cfg(test)]
#[path = "../../tests/unit/chat/lan_ip.rs"]
mod tests;
