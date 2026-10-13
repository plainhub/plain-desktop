use anyhow::Result;
use plain_server::{
    assembly::headless::{HeadlessService, Options},
    platform::system::consts::AppPaths,
};

pub async fn run(paths: &AppPaths, options: Options) -> Result<()> {
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut service = HeadlessService::start(paths, options).await?;
    let result = tokio::select! {
        result = shutdown_signal(
            #[cfg(unix)] &mut sigterm
        ) => result,
        result = service.wait() => result,
    };
    service.shutdown().await;
    result
}

async fn shutdown_signal(#[cfg(unix)] sigterm: &mut tokio::signal::unix::Signal) -> Result<()> {
    #[cfg(unix)]
    tokio::select! {
        _ = sigterm.recv() => Ok(()),
        result = tokio::signal::ctrl_c() => result.map_err(Into::into),
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.map_err(Into::into)
}
