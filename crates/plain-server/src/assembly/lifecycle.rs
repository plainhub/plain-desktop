use crate::assembly::server::PublicServer;
use crate::assembly::server::ServerState;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

#[derive(Clone, serde::Serialize)]
pub(crate) struct HttpServerFailure {
    pub generation: u64,
    pub message: String,
}

pub(crate) fn next_generation() -> u64 {
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    GENERATION.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn monitor(
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
        let failure = HttpServerFailure {
            generation,
            message: error,
        };
        *state.http_failure.lock().unwrap() = Some(failure.clone());
        let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::events::HTTP_SERVER_FAILED,
            serde_json::to_string(&failure).unwrap(),
        ));
        drop(guard);
    });
}

pub(crate) async fn stop_locked(state: &ServerState, guard: &mut Option<PublicServer>) {
    let _control = state.peer_status.control.lock().await;
    state
        .peer_status
        .public_active
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state.peer_status.outgoing.stop().await;
    state.main_ws.close_client(state, None);
    #[cfg(feature = "mobile")]
    state.mobile.mms.cancel_all().await;
    let _cast_guard = state.cast.operations.lock().await;
    crate::services::casting::dlna_sender_playback::end(state, true).await;
    let _receiver_guard = state.cast.receiver_operations.lock().await;
    state.dlna.stop().await;
    crate::services::casting::dlna_sender_runtime::release_permission(
        state,
        &state.cast.receiver_lease,
    )
    .await;
    state.cast.stop_tasks();
    crate::services::casting::dlna_sender_runtime::release_permission(
        state,
        &state.cast.scan_lease,
    )
    .await;
    state.cast.publish(state);
    if let Some(public) = guard.take() {
        let _ = public.stop.send(true);
        let PublicServer { listeners, .. } = public;
        listeners.shutdown().await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/assembly/lifecycle.rs"]
mod tests;
