//! Subcommand implementations. Mirrors the `cmd/*.go` files from the Go
//! project. Each module exposes a single `run` function.

pub mod bench;
pub mod install;
pub mod passwd;
pub mod run;
pub mod uninstall;
pub mod update;

#[derive(Parser, Debug)]
#[command(
    name = "plain-nas",
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
