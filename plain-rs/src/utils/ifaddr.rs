//! Enumerate local network interfaces via `getifaddrs(3)`.
//!
//! Ported from plain-nas `src/ifaddr_local.rs`; the dependency-free
//! replacement for the `if-addrs` crate, which wrapped the same syscall. The
//! Unix implementation calls `getifaddrs(3)` via `libc`. That is a POSIX API
//! with no Windows equivalent in `libc`, so on non-Unix targets the lists come
//! back empty and call sites must tolerate that anyway (no network is legal).

use std::net::{Ipv4Addr, Ipv6Addr};

/// One IPv4 interface entry. `netmask` is `None` if the kernel reported no
/// netmask (uncommon but legal — e.g. point-to-point links).
#[derive(Debug, Clone)]
pub struct Ifv4 {
    pub name: String,
    pub ip: Ipv4Addr,
    pub netmask: Option<Ipv4Addr>,
}

/// One IPv6 interface entry. `index` is the kernel's interface index, which
/// multicast joins need; it is `0` only when the name cannot be resolved.
#[derive(Debug, Clone)]
pub struct Ifv6 {
    pub name: String,
    pub ip: Ipv6Addr,
    pub index: u32,
}

/// Every up, non-loopback IPv4 interface.
pub fn list() -> Vec<Ifv4> {
    interfaces().0
}

/// Every up, non-loopback IPv6 interface that is routable off-link.
///
/// Link-local (`fe80::/10`) addresses are left out, which is what the
/// `if-addrs` crate did by default and what the mDNS caller was written
/// against: they are only meaningful together with their interface, and
/// feeding them in would make the responder announce itself on every VPN
/// and AWDL interface the machine happens to have.
pub fn list_v6() -> Vec<Ifv6> {
    interfaces()
        .1
        .into_iter()
        .filter(|iface| !is_link_local(&iface.ip))
        .collect()
}

fn is_link_local(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xffc0 == 0xfe80
}

#[cfg(unix)]
fn interfaces() -> (Vec<Ifv4>, Vec<Ifv6>) {
    let (mut v4, mut v6) = (Vec::new(), Vec::new());
    let mut raw: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` writes a heap-allocated linked list (or NULL on
    // failure, handled below) to its out-param. We free it after the walk.
    let rc = unsafe { libc::getifaddrs(&mut raw) };
    if rc != 0 {
        log::warn!(
            "[ifaddr] getifaddrs failed: {}",
            std::io::Error::last_os_error()
        );
        return (v4, v6);
    }
    // SAFETY: `raw` is a valid pointer to a linked list (or NULL on alloc
    // failure; we've already early-returned for that). We walk it exactly
    // once, and free it after the walk.
    let mut cur = std::ptr::NonNull::new(raw);
    while let Some(node) = cur {
        let ifa = unsafe { node.as_ref() };
        let addr = ifa.ifa_addr as *const libc::sockaddr;
        let netmask = ifa.ifa_netmask as *const libc::sockaddr;
        let up = (ifa.ifa_flags & libc::IFF_UP as libc::c_uint) != 0;
        let loopback = (ifa.ifa_flags & libc::IFF_LOOPBACK as libc::c_uint) != 0;
        let name = ifa.ifa_name;
        if !addr.is_null() && up && !loopback && !name.is_null() {
            // SAFETY: the kernel hands us a NUL-terminated name.
            let name = unsafe { std::ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned();
            // SAFETY: reading sa_family through a non-null sockaddr pointer.
            match unsafe { (*addr).sa_family } as libc::c_int {
                libc::AF_INET => {
                    // SAFETY: the family says this is a sockaddr_in.
                    let ip = Ipv4Addr::from(u32::from_be(unsafe {
                        (*(addr as *const libc::sockaddr_in)).sin_addr.s_addr
                    }));
                    let mask = if netmask.is_null() {
                        None
                    } else {
                        // SAFETY: an IPv4 netmask beside an AF_INET address
                        // is a sockaddr_in.
                        Some(Ipv4Addr::from(u32::from_be(unsafe {
                            (*(netmask as *const libc::sockaddr_in)).sin_addr.s_addr
                        })))
                    };
                    v4.push(Ifv4 {
                        name,
                        ip,
                        netmask: mask,
                    });
                }
                libc::AF_INET6 => {
                    // SAFETY: the family says this is a sockaddr_in6.
                    let sin6 = unsafe { &*(addr as *const libc::sockaddr_in6) };
                    let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                    // Link-local addresses carry their interface in the
                    // scope id; anything else needs the name resolved.
                    let index = if sin6.sin6_scope_id != 0 {
                        sin6.sin6_scope_id
                    } else {
                        index_of(&name)
                    };
                    v6.push(Ifv6 { name, ip, index });
                }
                _ => {}
            }
        }
        cur = std::ptr::NonNull::new(ifa.ifa_next);
    }
    // SAFETY: matches the `getifaddrs` above.
    unsafe { libc::freeifaddrs(raw) };
    (v4, v6)
}

#[cfg(unix)]
fn index_of(name: &str) -> u32 {
    let Ok(name) = std::ffi::CString::new(name) else {
        return 0;
    };
    // SAFETY: `name` is a valid NUL-terminated C string for the duration of
    // the call; the kernel only reads it.
    unsafe { libc::if_nametoindex(name.as_ptr()) }
}

#[cfg(not(unix))]
fn interfaces() -> (Vec<Ifv4>, Vec<Ifv6>) {
    log::warn!("[ifaddr] getifaddrs(3) is unavailable on this platform");
    (Vec::new(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_are_well_formed_or_empty() {
        // No network in CI is possible; the contract is that nothing panics
        // and loopback never shows up.
        for iface in list() {
            assert!(!iface.ip.is_loopback());
            assert!(!iface.name.is_empty());
        }
        for iface in list_v6() {
            assert!(!iface.ip.is_loopback());
            assert!(!iface.name.is_empty());
            assert!(!is_link_local(&iface.ip), "{} {}", iface.name, iface.ip);
        }
    }

    #[test]
    fn a_v6_index_matches_what_the_kernel_reports_for_that_name() {
        for iface in interfaces().1 {
            if iface.index == 0 {
                continue;
            }
            assert_eq!(iface.index, index_of(&iface.name), "{}", iface.name);
        }
    }
}
