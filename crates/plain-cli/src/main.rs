use anyhow::Result;

mod bootstrap;
mod cli;
mod version;

fn main() -> Result<()> {
    bootstrap::run()
}
