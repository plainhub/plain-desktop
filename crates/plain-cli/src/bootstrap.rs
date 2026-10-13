use anyhow::Result;
use clap::Parser;

pub fn run() -> Result<()> {
    plain_server::assembly::headless::initialize_tls();

    plain_server::platform::system::version::set(
        crate::version::VERSION,
        crate::version::COMMIT,
        crate::version::BUILD_TIME,
    );
    plain_server::platform::system::log::init("info");

    let cli = crate::cli::Cli::parse();
    let paths = cli.paths()?;

    match cli.command {
        crate::cli::Cmd::Install { with_libreoffice } => crate::cli::install::run(with_libreoffice),
        crate::cli::Cmd::Uninstall {
            keep_config,
            keep_data,
        } => crate::cli::uninstall::run(&paths, keep_config, keep_data),
        crate::cli::Cmd::Passwd { stdin } => crate::cli::passwd::run(&paths, stdin),
        crate::cli::Cmd::Update => crate::cli::update::run(&paths),
        crate::cli::Cmd::Bench => crate::cli::bench::run(&paths),
        crate::cli::Cmd::Run {
            media_directories,
            web_root,
            automount,
        } => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(crate::cli::run::run(
                &paths,
                plain_server::assembly::headless::Options {
                    media_directories,
                    web_root,
                    automount,
                },
            ))
        }
    }
}
