//! plain-nas (Rust rewrite of plainnas)

use anyhow::Result;
use clap::{Parser, Subcommand};

#[macro_use]
mod macros;
mod api;
mod app_update;
mod automount;
mod blockdev;
mod chat;
mod chunked_upload;
mod cmd;
mod consts;
mod crypto;
mod device_info;
mod devtools_sqlite;
mod dlna;
mod format_disk;
pub mod gql;
mod library;
mod log;
mod mounts;
#[allow(dead_code)]
mod pdf_preview;
mod prefs;
mod read_password;
mod samba;
mod storage_disks;
mod temp_store;
mod version;
mod ws_hub;
mod xml_sax;

// Media/file stack — now hosted by plain-rs (`plain_rs::media`), wired
// through under the historical module paths so call sites stay stable.
pub use plain_rs::media::config;
pub use plain_rs::media::eventbus;
pub use plain_rs::media::kv as db;
pub use plain_rs::media::fsx;
pub use plain_rs::media::file_tasks;
pub use plain_rs::media::scan as media_scan;
pub use plain_rs::media::mountinfo;
pub use plain_rs::media::search;
pub use plain_rs::media::index as search_index;
pub use plain_rs::media::trash;
pub use plain_rs::media::uuid as media_uuid;
pub use plain_rs::media::walk;
pub use plain_rs::media::watcher;

/// `crate::media::…` keeps resolving: the search index is now
/// `image_index` in plain-rs, the thumbnail engine `thumb`.
pub mod media {
    pub use plain_rs::media::cover;
    pub use plain_rs::media::image_index as search_index;
    pub use plain_rs::media::lyrics;
    pub use plain_rs::media::metadata;
    pub use plain_rs::media::thumb as thumb_engine;
    pub use plain_rs::media::video;
}

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
