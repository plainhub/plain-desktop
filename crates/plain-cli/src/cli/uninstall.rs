use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

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
