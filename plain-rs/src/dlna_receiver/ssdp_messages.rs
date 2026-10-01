pub const SSDP_ADDR: &str = "239.255.255.250";
pub const SSDP_PORT: u16 = 1900;
pub const DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
pub const AVT_TYPE: &str = "urn:schemas-upnp-org:service:AVTransport:1";

pub fn alive_messages(uuid: &str, ip: &str, port: u16) -> Vec<String> {
    vec![
        notify_msg(uuid, "upnp:rootdevice", "ssdp:alive", ip, port),
        notify_msg(
            &format!("{uuid}::{DEVICE_TYPE}"),
            DEVICE_TYPE,
            "ssdp:alive",
            ip,
            port,
        ),
        notify_msg(
            &format!("{uuid}::{AVT_TYPE}"),
            AVT_TYPE,
            "ssdp:alive",
            ip,
            port,
        ),
    ]
}

pub fn byebye_messages(uuid: &str, ip: &str, port: u16) -> Vec<String> {
    vec![
        notify_msg(uuid, "upnp:rootdevice", "ssdp:byebye", ip, port),
        notify_msg(
            &format!("{uuid}::{DEVICE_TYPE}"),
            DEVICE_TYPE,
            "ssdp:byebye",
            ip,
            port,
        ),
        notify_msg(
            &format!("{uuid}::{AVT_TYPE}"),
            AVT_TYPE,
            "ssdp:byebye",
            ip,
            port,
        ),
    ]
}

pub fn search_responses(uuid: &str, ip: &str, port: u16) -> Vec<String> {
    vec![
        search_response(
            "upnp:rootdevice",
            &format!("{uuid}::upnp:rootdevice"),
            ip,
            port,
        ),
        search_response(DEVICE_TYPE, &format!("{uuid}::{DEVICE_TYPE}"), ip, port),
        search_response(AVT_TYPE, &format!("{uuid}::{AVT_TYPE}"), ip, port),
    ]
}

fn notify_msg(usn: &str, nt: &str, nts: &str, ip: &str, port: u16) -> String {
    format!(
        "NOTIFY * HTTP/1.1\r\nHOST: {SSDP_ADDR}:{SSDP_PORT}\r\n\
         CACHE-CONTROL: max-age=1800\r\nLOCATION: http://{ip}:{port}/description.xml\r\n\
         NT: {nt}\r\nNTS: {nts}\r\nSERVER: Android/1.0 UPnP/1.1 PlainApp/1.0\r\nUSN: {usn}\r\n\r\n"
    )
}

fn search_response(st: &str, usn: &str, ip: &str, port: u16) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\n\
         LOCATION: http://{ip}:{port}/description.xml\r\n\
         SERVER: Android/1.0 UPnP/1.1 PlainApp/1.0\r\nST: {st}\r\nUSN: {usn}\r\n\r\n"
    )
}
