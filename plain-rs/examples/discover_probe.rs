//! Standalone mDNS browse probe - runs the real `service_browser` against the
//! live network with a raw packet tap so discovery failures can be localized:
//! queries sent (loop-back copies), external packets received, instance state.

use plain_rs::mdns::host_responder;
use plain_rs::mdns::service_browser::{FoundDevice, MdnsServiceBrowser};
use std::sync::{Arc, RwLock};
use std::time::Duration;

struct PrintLogger;
impl log::Log for PrintLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        println!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: PrintLogger = PrintLogger;

fn main() {
    log::set_logger(&LOGGER).ok();
    log::set_max_level(log::LevelFilter::Debug);

    host_responder::add_packet_listener(Arc::new(|data: &[u8], sender: &str| {
        let external =
            !host_responder::is_local_ip(sender) && !host_responder::is_local_ipv6(sender);
        println!("[packet] {} len={} local={}", sender, data.len(), !external);
    }));

    let hostname = host_responder::normalize_hostname("probe-desktop");
    let browser = MdnsServiceBrowser::new(
        "probe-client-id".to_string(),
        Arc::new(RwLock::new(hostname.clone())),
        |d: FoundDevice| {
            println!(
                "[FOUND] name={} id={} ips={:?} ipv6={:?} port={} dv={}",
                d.name, d.id, d.ips, d.ipv6, d.port, d.device_type
            );
        },
    );
    let seeds: Vec<String> = std::env::args().skip(1).collect();
    if !seeds.is_empty() {
        println!("seeding known addrs: {:?}", seeds);
        browser.seed_known_addrs(&seeds);
    }
    println!("starting responder hostname={hostname}");
    host_responder::start(&hostname, None);
    browser.install_listener();
    browser.start();

    for i in 1..=8 {
        std::thread::sleep(Duration::from_secs(5));
        let snap = browser.snapshot();
        println!(
            "=== t={}s instances={} running={} ===",
            i * 5,
            snap.len(),
            browser.is_running()
        );
        for s in snap {
            println!(
                "   {} complete={} port={} ips={:?} v6={:?}",
                s.instance_name, s.complete, s.port, s.ips, s.ipv6
            );
        }
    }
    browser.stop();
}
