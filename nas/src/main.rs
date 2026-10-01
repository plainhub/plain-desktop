//! plain-nas (Rust rewrite of plainnas)

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;
mod crypto;
mod prefs;
mod read_password;
mod version;

// Chat stack — one assembly in plain-rs (`plain_rs::chat_service`,
// NAS flavor via `ChatState::nas_init`), re-exported under the
// historical module path so call sites stay stable.
pub use plain_rs::chat_service;

// Shared system/domain modules hosted by plain-rs, re-exported under the historical
// module paths so call sites stay stable.
pub use plain_rs::storage::automount;
pub use plain_rs::system::consts;
pub use plain_rs::system::log;
pub use plain_rs::storage::mounts;

// Media/file stack — now hosted by plain-rs (`plain_rs::media`), wired
// through under the historical module paths so call sites stay stable.
pub use plain_rs::media::config;
pub use plain_rs::media::kv as db;
pub use plain_rs::media::scan as media_scan;
pub use plain_rs::media::watcher;

use consts::AppPaths;

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

    plain_rs::system::version::set(
        crate::version::VERSION,
        crate::version::COMMIT,
        crate::version::BUILD_TIME,
    );
    crate::log::init("info");

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
