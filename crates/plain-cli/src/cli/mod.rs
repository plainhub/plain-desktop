pub mod bench;
pub mod install;
pub mod passwd;
pub mod run;
pub mod uninstall;
pub mod update;

#[derive(Parser, Debug)]
#[command(
    name = "plainapp",
    version,
    about = "Lightweight NAS for Linux (Rust rewrite)"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Cmd,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Cmd {
    Install {
        #[arg(long)]
        with_libreoffice: bool,
    },
    Uninstall {
        #[arg(long)]
        keep_config: bool,
        #[arg(long)]
        keep_data: bool,
    },
    Run,
    Passwd,
    Update,
    Bench,
}

use clap::{Parser, Subcommand};

#[cfg(test)]
#[path = "../../tests/unit/cli.rs"]
mod tests;
