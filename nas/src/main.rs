//! plain-nas (Rust rewrite of plainnas)

use anyhow::Result;

mod cli;
mod crypto;
mod host;
mod prefs;
mod read_password;
mod version;

mod bootstrap;

fn main() -> Result<()> {
    bootstrap::run()
}

pub mod platform;
