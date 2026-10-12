use super::types::{ScanProgress, ScanState};
use crate::assembly::server::ServerState;
use crate::graphql::executor::RequestOrigin;
use crate::native::media_catalog::NativeMediaCatalog;
use crate::services::files::media_scans::Status;
use async_graphql::{Context, Object, Result};

fn catalog(ctx: &Context<'_>) -> Result<NativeMediaCatalog> {
    if ctx.data_opt::<RequestOrigin>() != Some(&RequestOrigin::Local) {
        return Err("Local media scan required".into());
    }
    Ok(NativeMediaCatalog::new(ctx.data::<ServerState>()?.clone()))
}

async fn public_catalog(ctx: &Context<'_>) -> Result<NativeMediaCatalog> {
    let state = ctx.data::<ServerState>()?;
    crate::http::auth::public_gate::require(ctx, &state.prefs, &[super::public::STORAGE])?;
    if !crate::http::auth::public_gate::require_granted(ctx, &[super::public::STORAGE])
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?
    {
        return Err("no_permission".into());
    }
    Ok(NativeMediaCatalog::new(state.clone()))
}

fn progress(catalog: NativeMediaCatalog) -> Result<ScanProgress> {
    let Some(snapshot) = catalog.file_scan_status()? else {
        return Ok(ScanProgress {
            indexed: 0,
            pending: Some(0),
            total: Some(0),
            state: ScanState::IDLE,
            error: None,
        });
    };
    let indexed = i32::try_from(snapshot.processed_files)
        .map_err(|_| "Media progress exceeds GraphQL Int")?;
    let total = snapshot
        .total_files
        .map(i32::try_from)
        .transpose()
        .map_err(|_| "Media total exceeds GraphQL Int")?;
    let state = match snapshot.status {
        Status::Running => ScanState::RUNNING,
        Status::Paused => ScanState::PAUSED,
        Status::Cancelling | Status::Cancelled => ScanState::STOPPED,
        Status::Completed => ScanState::IDLE,
        Status::Failed => ScanState::FAILED,
    };
    Ok(ScanProgress {
        indexed,
        pending: total.map(|total| total.saturating_sub(indexed)),
        total,
        state,
        error: snapshot.error,
    })
}

#[derive(Default)]
pub struct MediaScanQuery;

#[Object]
impl MediaScanQuery {
    async fn scan_progress(&self, ctx: &Context<'_>) -> Result<ScanProgress> {
        progress(catalog(ctx)?)
    }

    async fn media_source_dirs(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        Ok(catalog(ctx)?.source_directories()?)
    }
}

#[derive(Default)]
pub struct MediaScanMutation;

#[Object]
impl MediaScanMutation {
    async fn start_media_scan(&self, ctx: &Context<'_>, root: String) -> Result<bool> {
        catalog(ctx)?.start_source_scan(&root)?;
        Ok(true)
    }

    async fn pause_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        catalog(ctx)?.pause_file_scan()?;
        Ok(true)
    }

    async fn resume_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        catalog(ctx)?.resume_file_scan()?;
        Ok(true)
    }

    async fn stop_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        catalog(ctx)?.cancel_file_scan()?;
        Ok(true)
    }

    async fn set_media_source_dirs(&self, ctx: &Context<'_>, dirs: Vec<String>) -> Result<bool> {
        catalog(ctx)?.set_source_directories(&dirs)?;
        Ok(true)
    }
}

#[derive(Default)]
pub struct MediaScanPublicQuery;

#[Object]
impl MediaScanPublicQuery {
    async fn scan_progress(&self, ctx: &Context<'_>) -> Result<ScanProgress> {
        progress(public_catalog(ctx).await?)
    }

    async fn media_source_dirs(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        Ok(public_catalog(ctx).await?.source_directories()?)
    }
}

#[derive(Default)]
pub struct MediaScanPublicMutation;

#[Object]
impl MediaScanPublicMutation {
    async fn start_media_scan(&self, ctx: &Context<'_>, root: String) -> Result<bool> {
        public_catalog(ctx).await?.start_source_scan(&root)?;
        Ok(true)
    }

    async fn pause_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        public_catalog(ctx).await?.pause_file_scan()?;
        Ok(true)
    }

    async fn resume_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        public_catalog(ctx).await?.resume_file_scan()?;
        Ok(true)
    }

    async fn stop_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        public_catalog(ctx).await?.cancel_file_scan()?;
        Ok(true)
    }

    async fn set_media_source_dirs(&self, ctx: &Context<'_>, dirs: Vec<String>) -> Result<bool> {
        public_catalog(ctx).await?.set_source_directories(&dirs)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/media/scan.rs"]
mod tests;
