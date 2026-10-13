use crate::prefs::Prefs;

pub fn http_port(prefs: &Prefs) -> u16 {
    prefs.get_user_or(super::user::keys::HTTP_PORT, 8080u64) as u16
}

pub fn set_http_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set_user(super::user::keys::HTTP_PORT, port as u64);
}

pub fn https_port(prefs: &Prefs) -> u16 {
    prefs.get_user_or(super::user::keys::HTTPS_PORT, 8443u64) as u16
}

pub fn set_https_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set_user(super::user::keys::HTTPS_PORT, port as u64);
}

#[cfg(test)]
#[path = "../../tests/unit/prefs/server.rs"]
mod tests;
