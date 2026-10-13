#[path = "install/deps.rs"]
mod deps;
#[path = "install/packages.rs"]
mod packages;
#[path = "install/ui.rs"]
mod ui;

use anyhow::Result;

pub fn run(with_libreoffice: bool) -> Result<()> {
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

    write_systemd_unit()?;
    println!("plain-nas install: done");
    Ok(())
}

fn write_systemd_unit() -> Result<()> {
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
