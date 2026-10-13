pub mod bench;
pub mod install;
pub mod passwd;
pub mod run;
pub mod uninstall;
pub mod update;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "plainapp",
    version,
    about = "PlainApp server and command-line tools"
)]
pub(crate) struct Cli {
    #[arg(long, global = true)]
    pub(crate) data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) cache_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) tls_cert: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) tls_key: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Cmd,
}

impl Cli {
    pub(crate) fn paths(&self) -> anyhow::Result<plain_server::platform::system::consts::AppPaths> {
        let mut paths =
            plain_server::platform::system::consts::AppPaths::detect_with_data_directory(
                self.data_dir.clone(),
            );
        if let Some(path) = &self.cache_dir {
            paths.cache_dir = path.clone();
        }
        if let Some(path) = &self.tls_cert {
            paths.tls_cert = path.clone();
        }
        if let Some(path) = &self.tls_key {
            paths.tls_key = path.clone();
        }
        paths.data_dir = std::path::absolute(paths.data_dir)?;
        paths.cache_dir = std::path::absolute(paths.cache_dir)?;
        paths.tls_cert = std::path::absolute(paths.tls_cert)?;
        paths.tls_key = std::path::absolute(paths.tls_key)?;
        Ok(paths)
    }
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
    Run {
        #[arg(long = "media-dir")]
        media_directories: Vec<PathBuf>,
        #[arg(long)]
        web_root: Option<PathBuf>,
        #[arg(long, help = "Mount Linux USB volumes automatically (requires root)")]
        automount: bool,
    },
    Passwd {
        #[arg(long, help = "Read the new password from standard input")]
        stdin: bool,
    },
    Update,
    Bench,
}

#[cfg(test)]
#[path = "../../tests/unit/cli.rs"]
mod tests;
