use anyhow::Result;

#[cfg(target_os = "linux")]
pub fn run(with_libreoffice: bool) -> Result<()> {
    plain_rs::platform::linux::install::service::install(with_libreoffice)?;
    println!("plainapp install: done");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn run(_with_libreoffice: bool) -> Result<()> {
    anyhow::bail!(
        "System service installation is available on Linux; use plainapp run on this platform"
    )
}
