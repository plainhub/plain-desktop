pub mod read_password;

pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}
