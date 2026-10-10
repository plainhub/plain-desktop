//! `plain-nas update` - placeholder. The Go side shells out to a separate
//! `plain-nas-updater` binary; for the MVP we print a message and exit.
use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

pub fn run(_paths: &AppPaths) -> Result<()> {
    println!(
        "plain-nas update: not yet implemented in the Rust MVP. Use install.sh to reinstall the latest release."
    );
    Ok(())
}
