use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

pub fn run(_paths: &AppPaths) -> Result<()> {
    println!(
        "plainapp update: not yet implemented in the Rust MVP. Replace the executable with the latest release."
    );
    Ok(())
}
