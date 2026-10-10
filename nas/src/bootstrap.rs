use anyhow::Result;
use clap::Parser;
use plain_server::platform::system::consts::AppPaths;

pub fn run() -> Result<()> {
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

    let cli = crate::cli::Cli::parse();
    let paths = AppPaths::detect();

    match cli.command {
        crate::cli::Cmd::Install { with_libreoffice } => {
            crate::cli::install::run(&paths, with_libreoffice)
        }
        crate::cli::Cmd::Uninstall {
            keep_config,
            keep_data,
        } => crate::cli::uninstall::run(&paths, keep_config, keep_data),
        crate::cli::Cmd::Passwd => crate::cli::passwd::run(&paths),
        crate::cli::Cmd::Update => crate::cli::update::run(&paths),
        crate::cli::Cmd::Bench => crate::cli::bench::run(&paths),
        crate::cli::Cmd::Run => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(crate::cli::run::run(&paths))
        }
    }
}
