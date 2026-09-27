//! Enumerate local IPv4 interfaces with their netmasks via
//! `getifaddrs(3)`. Ported from plain-nas `src/ifaddr_local.rs`.
//!
//! A dependency-free alternative to the `if-addrs` crate for consumers
//! that want `ip` + `netmask` (e.g. broadcast computation in SSDP)
//! without an extra FFI wrapper. The `mdns` module still uses the
//! `if-addrs` crate internally; this module is the shared primitive for
//! everything else.
//!
//! The Unix implementation calls `getifaddrs(3)` via `libc`. That is a
//! POSIX API with no Windows equivalent in `libc`, so on non-Unix
//! targets `list()` logs a warning and returns an empty `Vec` — call
//! sites must tolerate an empty list anyway (no network is legal).

use std::net::Ipv4Addr;

#[cfg(unix)]
use std::ptr::NonNull;

/// One IPv4 interface entry. `netmask` is `None` if the kernel reported
/// no netmask (uncommon but legal — e.g. point-to-point links).
#[derive(Debug, Clone, Copy)]
pub struct Ifv4 {
    pub ip: Ipv4Addr,
    pub netmask: Option<Ipv4Addr>,
}

/// Walk the kernel's interface list, returning every up, non-loopback
/// IPv4 interface. Errors are logged (and an empty `Vec` is returned)
/// so the call site stays panic-free.
#[cfg(unix)]
pub fn list() -> Vec<Ifv4> {
    let mut out = Vec::new();
    let mut raw: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` writes a heap-allocated linked list of
    // `ifaddrs` structs to its out-param. The caller must free it with
    // `freeifaddrs`. We always do.
    let rc = unsafe { libc::getifaddrs(&mut raw) };
    if rc != 0 {
        log::warn!(
            "[ifaddr] getifaddrs failed: {}",
            std::io::Error::last_os_error()
        );
        return out;
    }
    // SAFETY: `raw` is a valid pointer to a linked list (or NULL on
    // alloc failure; we've already early-returned for that). We walk it
    // exactly once, and free it after the walk.
    let mut cur = NonNull::new(raw);
    while let Some(node) = cur {
        // SAFETY: node came from `getifaddrs` and the next-pointer is
        // valid for as long as we haven't called `freeifaddrs`.
        let ifa = unsafe { node.as_ref() };
        let addr = ifa.ifa_addr as *const libc::sockaddr;
        let netmask = ifa.ifa_netmask as *const libc::sockaddr;

        // Only IPv4 (sa_family == AF_INET) and only "up" + not loopback.
        if !addr.is_null()
            && unsafe { (*addr).sa_family } == libc::AF_INET as libc::sa_family_t
            && (ifa.ifa_flags & libc::IFF_UP as libc::c_uint) != 0
            && (ifa.ifa_flags & libc::IFF_LOOPBACK as libc::c_uint) == 0
        {
            // SAFETY: addr is non-null and sa_family is AF_INET, so
            // casting to `sockaddr_in` is sound.
            let ip_u32 =
                u32::from_be(unsafe { (*(addr as *const libc::sockaddr_in)).sin_addr.s_addr });
            let mask = if netmask.is_null() {
                None
            } else {
                let m = u32::from_be(unsafe {
                    (*(netmask as *const libc::sockaddr_in)).sin_addr.s_addr
                });
                Some(Ipv4Addr::from(m))
            };
            out.push(Ifv4 {
                ip: Ipv4Addr::from(ip_u32),
                netmask: mask,
            });
        }
        cur = NonNull::new(ifa.ifa_next);
    }
    // SAFETY: matches the `getifaddrs` above.
    unsafe { libc::freeifaddrs(raw) };
    out
}

#[cfg(not(unix))]
pub fn list() -> Vec<Ifv4> {
    log::warn!("[ifaddr] getifaddrs(3) is unavailable on this platform");
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_returns_at_least_one_up_iface_or_empty() {
        // No network in CI is possible; the contract is only that it
        // must not panic and every loopback entry is filtered out.
        for iface in list() {
            assert!(!iface.ip.is_loopback());
        }
    }
}
