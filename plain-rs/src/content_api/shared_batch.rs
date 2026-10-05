use super::server::ServerState;
use crate::shares::{
    batch_plan::{Kind, Plan, Walker},
    batch_queue::{Intent, Queue, Receipt, Run},
};
use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Default)]
pub(super) struct Runtime {
    pub queue: Queue,
    workers: Mutex<HashMap<String, (String, Option<JoinHandle<()>>, Arc<AtomicBool>)>>,
}
impl Runtime {
    pub(super) async fn shutdown(&self, host: &super::host::Host) {
        self.queue.stop();
        let mut workers = self.workers.lock().await;
        for (_, (generation, job, clean)) in workers.drain() {
            if let Some(job) = job {
                job.abort();
                let _ = job.await;
            }
            if !clean.load(Ordering::SeqCst) {
                let _ = host
                    .call("sharedTransferCancel", json!({"generation":generation}))
                    .await;
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Enqueue {
        intent: Intent,
    },
    Control {
        id: String,
        command: String,
    },
    Snapshot,
    Progress {
        id: String,
        generation: String,
        ticket: String,
        bytes: i64,
    },
    Receipt {
        id: String,
        generation: String,
        ticket: String,
        receipt: Receipt,
    },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if *state.stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let work: Result<Value> = async {
        Ok(match request {
            Request::Enqueue { intent } => json!(state.shared_batches.queue.enqueue(intent)?),
            Request::Snapshot => state.shared_batches.queue.snapshot(),
            Request::Progress {
                id,
                generation,
                ticket,
                bytes,
            } => json!(
                state
                    .shared_batches
                    .queue
                    .progress(&id, &generation, &ticket, bytes)?
            ),
            Request::Receipt {
                id,
                generation,
                ticket,
                receipt,
            } => json!(
                state
                    .shared_batches
                    .queue
                    .receipt(&id, &generation, &ticket, receipt)?
            ),
            Request::Control { id, command } => {
                let mut workers = state.shared_batches.workers.lock().await;
                if let Some((generation, worker, clean)) = workers.get_mut(&id) {
                    if !matches!(command.as_str(), "pause" | "cancel" | "remove") {
                        ensure!(
                            !state.shared_batches.queue.is_active(&id),
                            "Shared batch still active"
                        );
                    }
                    let paused = state.shared_batches.queue.control(&id, "pause")?.is_some();
                    if let Some(job) = worker.take() {
                        job.abort();
                        let _ = job.await;
                    }
                    if !clean.load(Ordering::SeqCst) {
                        let result = state
                            .host
                            .call("sharedTransferCancel", json!({"generation":generation}))
                            .await
                            .map_err(anyhow::Error::msg)?;
                        ensure!(
                            result == json!(true),
                            "OS adapter did not cancel shared transfer"
                        );
                    }
                    workers.remove(&id);
                    if command == "pause" {
                        return Ok(json!(paused));
                    }
                    if command == "retry" && paused {
                        return Ok(json!(
                            state.shared_batches.queue.control(&id, "resume")?.is_some()
                        ));
                    }
                }
                json!(state.shared_batches.queue.control(&id, &command)?.is_some())
            }
        })
    }
    .await;
    match work {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
pub(super) fn start(state: ServerState) {
    let progress_state = state.clone();
    tokio::spawn(async move {
        let mut changed = progress_state.shared_batches.queue.changed.subscribe();
        let mut stop = progress_state.stop.clone();
        loop {
            tokio::select! { result=changed.changed()=>if result.is_err(){break}, _=stop.changed()=>break };
            let _ = progress_state
                .events
                .send(crate::ws_event::WsEvent::broadcast(10004, String::new()));
        }
    });

    let mut changed = state.shared_batches.queue.changed.subscribe();
    let mut stop = state.stop.clone();
    tokio::spawn(async move {
        loop {
            {
                let mut workers = state.shared_batches.workers.lock().await;
                workers.retain(|id, (_, _, clean)| {
                    !clean.load(Ordering::SeqCst) || state.shared_batches.queue.is_active(id)
                });
                while workers.len() < 3 {
                    let Some(run) = state
                        .shared_batches
                        .queue
                        .claim(&workers.keys().cloned().collect())
                    else {
                        break;
                    };
                    let id = run.snapshot.id.clone();
                    let generation = run.snapshot.generation.clone();
                    let worker_state = state.clone();
                    let cleaned = Arc::new(AtomicBool::new(false));
                    let worker_cleaned = cleaned.clone();
                    let job = tokio::spawn(async move {
                        let result = execute(&worker_state, &run).await;
                        let cleanup = worker_state
                            .host
                            .call(
                                "sharedTransferCancel",
                                json!({"generation":run.snapshot.generation}),
                            )
                            .await;
                        worker_cleaned.store(
                            matches!(&cleanup,Ok(value) if value==&json!(true)),
                            Ordering::SeqCst,
                        );
                        let error = result
                            .err()
                            .map(|e| e.to_string())
                            .or_else(|| match cleanup {
                                Ok(v) if v == json!(true) => None,
                                Ok(_) => Some("OS adapter did not release shared transfer".into()),
                                Err(e) => Some(e),
                            });
                        worker_state.shared_batches.queue.finish(
                            &run.snapshot.id,
                            &run.snapshot.generation,
                            error,
                        );
                    });
                    workers.insert(id, (generation, Some(job), cleaned));
                }
            }
            tokio::select! {_=stop.changed()=>break, result=changed.changed()=>if result.is_err(){break}}
        }
        state.shared_batches.shutdown(&state.host).await;
    });
}
async fn plan(state: &ServerState, run: &Run) -> Result<Plan> {
    let mut walker = Walker::new(
        run.intent.kind,
        run.intent.entries.clone(),
        &run.intent.target_dir,
        &run.intent.downloads_base,
    )?;
    while let Some(path) = walker.next_directory()? {
        let info = super::shared_client::fetch(
            state,
            &run.intent.link,
            if path.is_empty() { None } else { Some(&path) },
        )
        .await?;
        walker.supply(info.entries)?;
    }
    walker.finish()
}
async fn operation(
    state: &ServerState,
    run: &Run,
    name: &str,
    size: Option<i64>,
    params: Value,
) -> Result<Receipt> {
    let (ticket, receiver) =
        state
            .shared_batches
            .queue
            .begin(&run.snapshot.id, &run.snapshot.generation, name, size)?;
    let accepted = state.host.call("sharedTransferStart", json!({"id":run.snapshot.id,"generation":run.snapshot.generation,"ticket":ticket,"operation":params})).await.map_err(anyhow::Error::msg)?;
    ensure!(
        accepted == json!(true),
        "OS adapter refused shared transfer"
    );
    let receipt = tokio::time::timeout(Duration::from_secs(15 * 60), receiver).await??;
    if let Some(error) = &receipt.error {
        return Err(anyhow::anyhow!(error.clone()));
    }
    Ok(receipt)
}
async fn execute(state: &ServerState, run: &Run) -> Result<()> {
    let plan = match &run.plan {
        Some(plan) => plan.clone(),
        None => plan(state, run).await?,
    };
    state
        .shared_batches
        .queue
        .planned(&run.snapshot.id, &run.snapshot.generation, plan.clone())?;
    let mut items = vec![];
    for target in &plan.targets {
        if run.intent.kind != Kind::Zip && run.completed.contains(&target.entry.virtual_path) {
            continue;
        }
        let url =
            run.intent
                .link
                .file_url(&run.intent.url_token, &target.entry.virtual_path, false)?;
        let result = operation(
            state,
            run,
            &target.entry.name,
            Some(target.entry.size),
            json!({"kind":"file","url":url,"target":target,"temporary":run.intent.kind==Kind::Zip}),
        )
        .await;
        match result {
            Ok(receipt) => {
                if run.intent.kind == Kind::Zip {
                    ensure!(
                        std::path::Path::new(&receipt.path).is_absolute(),
                        "Invalid OS temporary path"
                    );
                    items.push(json!({"sourcePath":receipt.path,"entryName":target.entry_name}));
                }
                state.shared_batches.queue.file_finished(
                    &run.snapshot.id,
                    &run.snapshot.generation,
                    &target.entry.virtual_path,
                    None,
                )?;
            }
            Err(error) => {
                state.shared_batches.queue.file_finished(
                    &run.snapshot.id,
                    &run.snapshot.generation,
                    &target.entry.virtual_path,
                    Some(error.to_string()),
                )?;
                let canceled = state
                    .host
                    .call(
                        "sharedTransferCancel",
                        json!({"generation":run.snapshot.generation}),
                    )
                    .await
                    .map_err(anyhow::Error::msg)?;
                ensure!(
                    canceled == json!(true),
                    "OS adapter did not cancel failed transfer"
                );
                if run.intent.kind == Kind::Zip {
                    return Err(error);
                }
            }
        }
    }
    if run.intent.kind == Kind::Zip {
        ensure!(!items.is_empty(), "No shared files to archive");
        operation(state,run,"",None,json!({"kind":"zip","items":items,"path":format!("{}/{}",run.intent.downloads_base.trim_end_matches('/'),run.intent.zip_name)})).await?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/shared_batch.rs"]
mod tests;
