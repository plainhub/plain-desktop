//! `plainnas install` - writes `/etc/plainnas/config.toml`, the
//! systemd unit, and installs the OS-level dependencies
//! (ffmpeg, samba + samba-vfs-modules, avahi).

#[path = "install/deps.rs"]
mod deps;
#[path = "install/packages.rs"]
mod packages;
#[path = "install/ui.rs"]
mod ui;

use crate::consts::AppPaths;
use anyhow::{Context, Result};
use std::path::Path;

pub fn run(paths: &AppPaths, with_libreoffice: bool) -> Result<()> {
    if !nix::unistd::Uid::current().is_root() {
        eprintln!("This command requires root privileges. Please run with sudo.");
        std::process::exit(1);
    }

    // 1. Install OS-level dependencies (ffmpeg, samba, avahi).
    //    Errors here are non-fatal — the user may have installed them
    //    manually already, or may be on a distro we don't know about.
    //    The Go side follows the same pattern (print_fail but continue).
    deps::install_all_deps();
    if with_libreoffice {
        deps::install_libre_office();
    }

    // 2. Write config + systemd unit.
    if let Some(parent) = paths.config_path.parent() {
        std::fs::create_dir_all(parent).context("mkdir /etc/plainnas")?;
    }
    let config = include_str!("install/config.toml");
    let config = config.replace(
        "__WITH_LO__",
        if with_libreoffice { "true" } else { "false" },
    );
    write_if_absent(&paths.config_path, &config)?;
    write_systemd_unit(paths)?;
    println!("plain-nas install: wrote {}", paths.config_path.display());
    Ok(())
}

fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, content).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_systemd_unit(_paths: &AppPaths) -> Result<()> {
    let unit_path = std::path::Path::new("/etc/systemd/system/plain-nas.service");
    if unit_path.exists() {
        return Ok(());
    }
    let binary = "/usr/local/bin/plain-nas";
    let unit = format!(
        r#"[Unit]
Description=plain-nas service
After=network-online.target smbd.service
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary} run
Restart=on-failure
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
"#,
    );
    std::fs::create_dir_all(unit_path.parent().unwrap()).ok();
    std::fs::write(unit_path, unit).ok();
    Ok(())
}
