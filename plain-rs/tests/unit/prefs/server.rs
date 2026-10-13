use super::*;

#[test]
fn server_ports_use_user_preferences_instead_of_system_values() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("system_prefs.json")).unwrap();
    assert_eq!(crate::prefs::server::http_port(&prefs), 8080);
    assert_eq!(crate::prefs::server::https_port(&prefs), 8443);
    prefs.set("http_port", 9090).unwrap();
    prefs.set("https_port", 9443).unwrap();
    assert_eq!(crate::prefs::server::http_port(&prefs), 8080);
    assert_eq!(crate::prefs::server::https_port(&prefs), 8443);
    crate::prefs::server::set_http_port(&prefs, 8180);
    crate::prefs::server::set_https_port(&prefs, 8543);
    let settings = crate::prefs::user::snapshot(&prefs).unwrap();
    assert_eq!(settings.http_port, 8180);
    assert_eq!(settings.https_port, 8543);
    assert_eq!(crate::prefs::server::http_port(&prefs), 8180);
    assert_eq!(crate::prefs::server::https_port(&prefs), 8543);
}
