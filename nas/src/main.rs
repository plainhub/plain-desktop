//! plain-nas (Rust rewrite of plainnas)

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;
mod crypto;
mod prefs;
mod read_password;
mod version;

use plain_server::platform::system::consts::AppPaths;

#[derive(Parser, Debug)]
#[command(
    name = "plain-nas",
    version,
    about = "Lightweight NAS for Linux (Rust rewrite)"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
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

fn main() -> Result<()> {
    // rustls needs an explicit crypto provider selection. We default to ring
    // (also enabled as a default feature of rustls) but install it manually
    // so the runtime can pick it up.
    let _ = rustls::crypto::ring::default_provider().install_default();

    plain_server::platform::system::version::set(
        crate::version::VERSION,
        crate::version::COMMIT,
        crate::version::BUILD_TIME,
    );
    plain_server::platform::system::log::init("info");

    let cli = Cli::parse();
    let paths = AppPaths::detect();

    match cli.command {
        Cmd::Install { with_libreoffice } => cmd::install::run(&paths, with_libreoffice),
        Cmd::Uninstall {
            keep_config,
            keep_data,
        } => cmd::uninstall::run(&paths, keep_config, keep_data),
        Cmd::Passwd => cmd::passwd::run(&paths),
        Cmd::Update => cmd::update::run(&paths),
        Cmd::Bench => cmd::bench::run(&paths),
        Cmd::Run => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(cmd::run::run(&paths))
        }
    }
}
