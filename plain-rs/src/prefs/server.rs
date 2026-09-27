use super::Prefs;

pub fn http_port(prefs: &Prefs) -> u16 {
    prefs.get_or("http_port", 8080u64) as u16
}

pub fn set_http_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set("http_port", port as u64);
}

pub fn https_port(prefs: &Prefs) -> u16 {
    prefs.get_or("https_port", 8443u64) as u16
}

pub fn set_https_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set("https_port", port as u64);
}
