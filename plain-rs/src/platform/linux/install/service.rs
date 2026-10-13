use anyhow::Result;
use std::path::Path;

pub const SERVICE_PATH: &str = "/etc/systemd/system/plain-nas.service";
pub const BINARY_PATH: &str = "/usr/local/bin/plainapp";

pub fn require_root() -> Result<()> {
    anyhow::ensure!(
        crate::platform::unix::is_root(),
        "This command requires root privileges. Please run with sudo."
    );
    Ok(())
}

pub fn install(with_libreoffice: bool) -> Result<()> {
    require_root()?;
    super::deps::install_all_deps();
    if with_libreoffice {
        super::deps::install_libre_office();
    }
    write_service(Path::new(SERVICE_PATH), BINARY_PATH)?;
    if super::packages::has_cmd("systemctl") {
        let status = std::process::Command::new("systemctl")
            .arg("daemon-reload")
            .status()?;
        anyhow::ensure!(status.success(), "systemd unit reload failed");
    }
    Ok(())
}

pub fn write_service(path: &Path, binary: &str) -> Result<()> {
    let unit = render_service(binary);
    if path.exists() && std::fs::read_to_string(path)? == unit {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, unit)?;
    Ok(())
}

fn render_service(binary: &str) -> String {
    format!(
        "[Unit]\nDescription=PlainApp service\nAfter=network-online.target smbd.service\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={binary} run\nRestart=on-failure\nRestartSec=5\nLimitNOFILE=65536\n\n[Install]\nWantedBy=multi-user.target\n"
    )
}

fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn uninstall(
    data: &Path,
    cert: &Path,
    key: &Path,
    keep_config: bool,
    keep_data: bool,
) -> Result<()> {
    require_root()?;
    if !keep_config {
        remove_file(cert)?;
        remove_file(key)?;
    }
    if !keep_data {
        match std::fs::remove_dir_all(data) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    remove_file(Path::new(SERVICE_PATH))
}

pub fn restart() -> Result<()> {
    let status = std::process::Command::new("systemctl")
        .args(["restart", "plain-nas.service"])
        .status()?;
    anyhow::ensure!(status.success(), "service restart failed");
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/platform/linux/install/service.rs"]
mod tests;
