use super::server::{PublicServer, ServerState};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

pub(super) fn next_generation() -> u64 {
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

pub(super) async fn health(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let guard = state.public_server.lock().await;
    let result = match guard.as_ref() {
        Some(server) => server.listeners.check_health().await,
        None => Err("Public HTTP server is not running".into()),
    };
    Json(serde_json::json!({"healthy": result.is_ok(), "error": result.err()})).into_response()
}

pub(super) fn monitor(
    state: ServerState,
    mut failure: watch::Receiver<Option<String>>,
    mut stop: watch::Receiver<bool>,
    generation: u64,
) {
    let mut core_stop = state.stop.clone();
    tokio::spawn(async move {
        let error = loop {
            if let Some(error) = failure.borrow().clone() {
                break error;
            }
            tokio::select! {
                biased;
                _ = core_stop.changed() => {
                    let mut guard = state.public_server.lock().await;
                    if guard.as_ref().map(|server| server.generation) == Some(generation) {
                        guard.take();
                    }
                    return;
                }
                _ = stop.changed() => return,
                changed = failure.changed() => {
                    if changed.is_err() { return; }
                }
            }
        };
        let mut guard = state.public_server.lock().await;
        if guard.as_ref().map(|server| server.generation) != Some(generation) {
            return;
        }
        stop_locked(&state, &mut guard).await;
        drop(guard);
        let _ = state
            .host
            .call(
                "mainGraphqlServerFailed",
                serde_json::json!({"generation": generation, "message": error}),
            )
            .await;
    });
}

pub(super) async fn stop_locked(state: &ServerState, guard: &mut Option<PublicServer>) {
    let _control = state.peer_status.control.lock().await;
    state
        .peer_status
        .public_active
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state.peer_status.outgoing.stop().await;
    state.main_ws.close_client(state, None);
    state.mms.cancel_all().await;
    let _cast_guard = state.cast.operations.lock().await;
    super::dlna_sender_playback::end(state, true).await;
    let _receiver_guard = state.cast.receiver_operations.lock().await;
    state.dlna.stop().await;
    super::dlna_sender_runtime::release_permission(state, &state.cast.receiver_lease).await;
    state.cast.stop_tasks();
    super::dlna_sender_runtime::release_permission(state, &state.cast.scan_lease).await;
    state.cast.publish(state);
    if let Some(public) = guard.take() {
        let _ = public.stop.send(true);
        let PublicServer { listeners, .. } = public;
        listeners.shutdown().await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_lifecycle.rs"]
mod tests;
