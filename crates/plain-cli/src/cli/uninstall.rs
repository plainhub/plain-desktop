use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

#[cfg(target_os = "linux")]
pub fn run(paths: &AppPaths, keep_config: bool, keep_data: bool) -> Result<()> {
    plain_rs::platform::linux::install::service::uninstall(
        &paths.data_dir,
        &paths.tls_cert,
        &paths.tls_key,
        keep_config,
        keep_data,
    )?;
    println!("plainapp uninstall: done");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn run(_paths: &AppPaths, _keep_config: bool, _keep_data: bool) -> Result<()> {
    anyhow::bail!("System service uninstallation is available on Linux")
}
