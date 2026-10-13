use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

pub async fn run(paths: &AppPaths) -> Result<()> {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut service = plain_server::assembly::nas::NasService::start(paths).await?;
    let result = tokio::select! {
        _ = sigterm.recv() => Ok(()),
        _ = sigint.recv() => Ok(()),
        result = service.wait() => result,
    };
    service.shutdown().await;
    result
}
