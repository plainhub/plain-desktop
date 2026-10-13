use anyhow::Result;
use std::process::Command;

pub fn set(name: &str) -> Result<()> {
    let status = Command::new("hostnamectl")
        .arg("set-hostname")
        .arg(name)
        .status()?;
    anyhow::ensure!(status.success(), "device_name_set_hostname_failed");
    let _ = Command::new("systemctl")
        .args(["try-restart", "avahi-daemon"])
        .status();
    Ok(())
}
