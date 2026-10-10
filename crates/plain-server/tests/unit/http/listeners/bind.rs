use super::*;
use std::net::TcpListener as StdTcpListener;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

fn grab_port() -> (StdTcpListener, u16) {
    let l = StdTcpListener::bind("0.0.0.0:0").expect("grab :0");
    let p = l.local_addr().unwrap().port();
    (l, p)
}

fn grab_loopback_port() -> (StdTcpListener, u16) {
    let l = StdTcpListener::bind("127.0.0.1:0").expect("grab loopback :0");
    let p = l.local_addr().unwrap().port();
    (l, p)
}

#[test]
fn bind_listener_succeeds_on_free_port() {
    let _guard = SERIAL.lock().unwrap();
    let (probe_hold, free_port) = grab_port();
    drop(probe_hold);
    let listener = bind_listener(free_port).expect("known-free fixed port should bind");
    assert_eq!(listener.local_addr().unwrap().port(), free_port);
}

#[test]
fn bind_listener_uses_os_assigned_port() {
    let _guard = SERIAL.lock().unwrap();
    let listener = bind_listener(0).expect("bind :0 should always succeed");
    assert_ne!(listener.local_addr().unwrap().port(), 0);
}

#[test]
fn bind_listener_fails_when_wildcard_taken() {
    let _guard = SERIAL.lock().unwrap();
    let (taken_hold, taken) = grab_port();
    let err = bind_listener(taken).unwrap_err();
    assert!(
        err.kind() == std::io::ErrorKind::AddrInUse
            || err.kind() == std::io::ErrorKind::PermissionDenied,
        "expected AddrInUse or PermissionDenied, got {:?}",
        err.kind()
    );
    drop(taken_hold);
}

#[test]
fn bind_listener_fails_when_loopback_taken() {
    let _guard = SERIAL.lock().unwrap();
    let (taken_hold, taken) = grab_loopback_port();
    let err = bind_listener(taken).unwrap_err();
    assert!(
        err.kind() == std::io::ErrorKind::AddrInUse
            || err.kind() == std::io::ErrorKind::PermissionDenied,
        "expected AddrInUse or PermissionDenied, got {:?}",
        err.kind()
    );
    drop(taken_hold);
}

#[test]
fn bind_listener_fallback_succeeds_when_port_taken() {
    let _guard = SERIAL.lock().unwrap();
    let (taken_hold, taken) = grab_port();
    let (free_hold, free) = grab_port();
    drop(free_hold);
    let l = bind_listener_fallback(taken, &[taken, free])
        .expect("fallback should bind the next free candidate");
    assert_eq!(l.local_addr().unwrap().port(), free);
    drop(taken_hold);
}

#[test]
fn bind_listener_fallback_prefers_free_configured_port() {
    let _guard = SERIAL.lock().unwrap();
    let (probe_hold, free_port) = grab_port();
    drop(probe_hold);
    let l = bind_listener_fallback(free_port, &[free_port])
        .expect("a free preferred port should be used");
    assert_eq!(l.local_addr().unwrap().port(), free_port);
}

#[test]
fn bind_listener_fallback_tries_preferred_port_outside_candidates() {
    let _guard = SERIAL.lock().unwrap();
    let (preferred_hold, preferred) = grab_port();
    let (candidate_hold, candidate) = grab_port();
    drop(preferred_hold);
    drop(candidate_hold);
    let listener = bind_listener_fallback(preferred, &[candidate])
        .expect("a free custom preferred port should be used");
    assert_eq!(listener.local_addr().unwrap().port(), preferred);
}

#[test]
fn bind_listener_fallback_uses_os_port_when_all_candidates_taken() {
    let _guard = SERIAL.lock().unwrap();
    let (a, pa) = grab_port();
    let (b, pb) = grab_port();
    let listener = bind_listener_fallback(pa, &[pa, pb])
        .expect("OS-assigned fallback should keep the server running");
    let picked = listener.local_addr().unwrap().port();
    assert_ne!(picked, pa);
    assert_ne!(picked, pb);
    drop(a);
    drop(b);
}

#[test]
fn bind_listener_fallback_handles_empty_candidates() {
    let _guard = SERIAL.lock().unwrap();
    let (preferred_hold, preferred) = grab_port();
    let listener = bind_listener_fallback(preferred, &[])
        .expect("empty candidate list should fall back to an OS-assigned port");
    let picked = listener.local_addr().unwrap().port();
    assert_ne!(picked, 0);
    assert_ne!(picked, preferred);
    drop(preferred_hold);
}
