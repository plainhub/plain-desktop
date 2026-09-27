//! `plain-nas uninstall` - removes config and/or data when not asked to keep.
use crate::consts::AppPaths;
use anyhow::Result;

pub fn run(paths: &AppPaths, keep_config: bool, keep_data: bool) -> Result<()> {
    if !nix::unistd::Uid::current().is_root() {
        eprintln!("This command requires root privileges. Please run with sudo.");
        std::process::exit(1);
    }
    if !keep_config {
        let _ = std::fs::remove_file(&paths.config_path);
        let _ = std::fs::remove_file(&paths.tls_cert);
        let _ = std::fs::remove_file(&paths.tls_key);
    }
    if !keep_data {
        let _ = std::fs::remove_dir_all(&paths.data_dir);
    }
    let _ = std::fs::remove_file("/etc/systemd/system/plain-nas.service");
    println!("plain-nas uninstall: done");
    Ok(())
}
