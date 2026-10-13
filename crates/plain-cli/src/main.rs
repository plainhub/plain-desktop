use anyhow::Result;

#[cfg(unix)]
mod bootstrap;
#[cfg(unix)]
mod cli;
#[cfg(unix)]
mod version;

#[cfg(unix)]
fn main() -> Result<()> {
    bootstrap::run()
}

#[cfg(not(unix))]
fn main() -> Result<()> {
    anyhow::bail!("The PlainApp server CLI requires a Unix host")
}
