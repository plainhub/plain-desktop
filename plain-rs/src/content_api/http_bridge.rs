use super::host::Host;
use axum::{
    body::Body,
    extract::{
        ConnectInfo, Multipart, Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};

#[derive(Clone)]
pub(super) struct RemoteHost(pub String);

const CHUNK: usize = 64 * 1024;
struct Pending {
    attached: oneshot::Sender<()>,
    outgoing: mpsc::Receiver<Message>,
    incoming: mpsc::Sender<Message>,
    stop: watch::Receiver<bool>,
}
pub struct HttpBridge {
    host: Arc<Host>,
    pending: Mutex<HashMap<String, Pending>>,
    capacity: Arc<Semaphore>,
}
struct Exchange {
    outgoing: mpsc::Sender<Message>,
    incoming: mpsc::Receiver<Message>,
    _permit: OwnedSemaphorePermit,
}
struct BodyForward(tokio::task::JoinHandle<()>);
impl Drop for BodyForward {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct Registration<'a> {
    bridge: &'a HttpBridge,
    id: String,
}
impl Drop for Registration<'_> {
    fn drop(&mut self) {
        self.bridge.pending.lock().unwrap().remove(&self.id);
    }
}
impl HttpBridge {
    pub fn new(host: Arc<Host>) -> Self {
        Self {
            host,
            pending: Mutex::new(HashMap::new()),
            capacity: Arc::new(Semaphore::new(32)),
        }
    }
    pub async fn attach(&self, id: &str, mut socket: WebSocket) {
        let pending = self.pending.lock().unwrap().remove(id);
        let Some(mut pending) = pending else {
            let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
            return;
        };
        let _ = pending.attached.send(());
        loop {
            tokio::select! {
                _ = pending.stop.changed() => break,
                message = pending.outgoing.recv() => {
                    let Some(message) = message else { break; };
                    if tokio::select! { _=pending.stop.changed()=>true, result=socket.send(message)=>result.is_err() } { break; }
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Ping(_)|Message::Pong(_))) => {},
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        Some(Ok(message)) => {
                            if tokio::select! {
                                _ = pending.stop.changed() => true,
                                result = pending.incoming.send(message) => result.is_err(),
                            } { break; }
                        }
                    }
                }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
    }
    async fn open(
        &self,
        metadata: Value,
        mut stop: watch::Receiver<bool>,
    ) -> Result<Exchange, StatusCode> {
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let id = uuid::Uuid::new_v4().to_string();
        let (attached_tx, attached_rx) = oneshot::channel();
        let (outgoing, receiver) = mpsc::channel(4);
        let (sender, incoming) = mpsc::channel(4);
        self.pending.lock().unwrap().insert(
            id.clone(),
            Pending {
                attached: attached_tx,
                outgoing: receiver,
                incoming: sender,
                stop: stop.clone(),
            },
        );
        let _registration = Registration {
            bridge: self,
            id: id.clone(),
        };
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = self.host.call("httpExchange", json!({"id":id})) => {
                result.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            }
        }
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = tokio::time::timeout(Duration::from_secs(15), attached_rx) => {
                result.map_err(|_| StatusCode::GATEWAY_TIMEOUT)?.map_err(|_| StatusCode::BAD_GATEWAY)?;
            }
        }
        let send = outgoing.send(Message::Text(metadata.to_string()));
        tokio::select! {
            _ = stop.changed() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            result = tokio::time::timeout(Duration::from_secs(15), send) => {
                result.map_err(|_| StatusCode::GATEWAY_TIMEOUT)?.map_err(|_| StatusCode::BAD_GATEWAY)?;
            }
        }
        Ok(Exchange {
            outgoing,
            incoming,
            _permit: permit,
        })
    }
}
#[derive(Clone)]
pub struct HttpBridgeState {
    pub bridge: Arc<HttpBridge>,
    pub stop: watch::Receiver<bool>,
}
fn text(value: Value) -> Message {
    Message::Text(value.to_string())
}
async fn send_chunks(sender: &mpsc::Sender<Message>, bytes: &[u8]) -> Result<(), String> {
    for chunk in bytes.chunks(CHUNK) {
        sender
            .send(Message::Binary(chunk.to_vec()))
            .await
            .map_err(|_| "host disconnected")?;
    }
    Ok(())
}
async fn forward_body(
    sender: mpsc::Sender<Message>,
    request: Request,
    multipart: bool,
) -> Result<(), String> {
    if multipart {
        use axum::extract::FromRequest;
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|e| e.to_string())?;
        while let Some(mut part) = multipart.next_field().await.map_err(|e| e.to_string())? {
            sender.send(text(json!({"kind":"part","name":part.name(),"filename":part.file_name(),"contentType":part.content_type()}))).await.map_err(|_| "host disconnected")?;
            while let Some(chunk) = part.chunk().await.map_err(|e| e.to_string())? {
                send_chunks(&sender, &chunk).await?;
            }
            sender
                .send(text(json!({"kind":"partEnd"})))
                .await
                .map_err(|_| "host disconnected")?;
        }
    } else {
        let mut body = request.into_body().into_data_stream();
        while let Some(chunk) = body.next().await {
            send_chunks(&sender, &chunk.map_err(|e| e.to_string())?).await?;
        }
    }
    sender
        .send(text(json!({"kind":"end"})))
        .await
        .map_err(|_| "host disconnected".to_string())
}
fn response_headers(response: &mut Response, packet: &Value) -> Result<(), StatusCode> {
    if let Some(headers) = packet.get("headers").and_then(Value::as_object) {
        for (name, values) in headers {
            let name = axum::http::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| StatusCode::BAD_GATEWAY)?;
            if matches!(
                name.as_str(),
                "connection" | "transfer-encoding" | "upgrade"
            ) {
                continue;
            }
            let values = values.as_array().ok_or(StatusCode::BAD_GATEWAY)?;
            response.headers_mut().remove(&name);
            for value in values {
                let value = value
                    .as_str()
                    .ok_or(StatusCode::BAD_GATEWAY)?
                    .parse()
                    .map_err(|_| StatusCode::BAD_GATEWAY)?;
                response.headers_mut().append(name.clone(), value);
            }
        }
    }
    Ok(())
}
pub async fn handle(
    State(state): State<HttpBridgeState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    scheme: axum::Extension<crate::http_transport::ConnectionScheme>,
    upgrade: Option<WebSocketUpgrade>,
    request: Request,
) -> Response {
    let head_timeout = if matches!(request.uri().path(), "/upload" | "/upload_chunk") {
        Duration::from_secs(15 * 60)
    } else {
        Duration::from_secs(30)
    };
    let metadata = json!({"kind":"request","method":if request.method()==axum::http::Method::HEAD {"GET"}else{request.method().as_str()},"scheme":scheme.0.0,"uri":request.uri().to_string(),"remoteHost":request.extensions().get::<RemoteHost>().map(|host|host.0.clone()).unwrap_or_else(||remote.ip().to_string()),"webSocket":upgrade.is_some(),"headers":request.headers().iter().map(|(k,v)|(k.as_str().to_string(),v.to_str().unwrap_or_default().to_string())).collect::<HashMap<_,_>>()});
    let multipart = request
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.to_ascii_lowercase().starts_with("multipart/form-data"));
    let headers = request.headers().clone();
    let mut exchange = match state.bridge.open(metadata, state.stop.clone()).await {
        Ok(e) => e,
        Err(s) => return s.into_response(),
    };
    let sender = exchange.outgoing.clone();
    let body_task = BodyForward(tokio::spawn(async move {
        if let Err(error) = forward_body(sender.clone(), request, multipart).await {
            let _ = sender
                .send(text(json!({"kind":"error","message":error})))
                .await;
        }
    }));
    let mut stop = state.stop.clone();
    let first = tokio::select! {
        _ = stop.changed() => None,
        result = tokio::time::timeout(head_timeout, exchange.incoming.recv()) => result.ok().flatten(),
    };
    body_task.0.abort();
    let Some(Message::Text(first)) = first else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    let Ok(packet) = serde_json::from_str::<Value>(&first) else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    match packet["kind"].as_str() {
        Some("upgrade") => match upgrade {
            Some(upgrade) => upgrade
                .on_upgrade(move |socket| relay_ws(socket, exchange, stop))
                .into_response(),
            None => StatusCode::BAD_GATEWAY.into_response(),
        },
        Some("file") => {
            let Some(path) = packet["path"].as_str() else {
                return StatusCode::BAD_GATEWAY.into_response();
            };
            let mut response = super::http_bridge_file::serve(path, &headers, &packet).await;
            if let Err(error) = response_headers(&mut response, &packet) {
                return error.into_response();
            }
            response
        }
        Some("response") => {
            let Ok(status) = packet["status"]
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or(())
                .and_then(|n| StatusCode::from_u16(n).map_err(|_| ()))
            else {
                return StatusCode::BAD_GATEWAY.into_response();
            };
            let stream = futures_util::stream::try_unfold(
                (exchange, stop),
                |(mut exchange, mut stop)| async move {
                    let message = tokio::select! { _=stop.changed()=>None, message=exchange.incoming.recv()=>message };
                    match message {
                        Some(Message::Binary(bytes)) => {
                            Ok::<_, std::io::Error>(Some((bytes, (exchange, stop))))
                        }
                        Some(Message::Text(message))
                            if serde_json::from_str::<Value>(&message)
                                .ok()
                                .is_some_and(|v| v["kind"] == "end") =>
                        {
                            Ok(None)
                        }
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionAborted,
                            "HTTP host stream interrupted",
                        )),
                    }
                },
            );
            let mut response = Body::from_stream(stream).into_response();
            *response.status_mut() = status;
            if let Err(error) = response_headers(&mut response, &packet) {
                return error.into_response();
            }
            response
        }
        _ => StatusCode::BAD_GATEWAY.into_response(),
    }
}
async fn relay_ws(mut socket: WebSocket, mut exchange: Exchange, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _=stop.changed()=>break,
            message=socket.next()=> {
                let message = match message {
                    Some(Ok(Message::Text(value)))=>text(json!({"kind":"wsText","text":value})),
                    Some(Ok(Message::Binary(bytes)))=>Message::Binary(bytes),
                    Some(Ok(Message::Ping(_)|Message::Pong(_)))=>continue,
                    _=>break,
                };
                if exchange.outgoing.send(message).await.is_err() {break;}
            },
            message=exchange.incoming.recv()=> {
                let message=match message {
                    Some(Message::Binary(bytes))=>Message::Binary(bytes),
                    Some(Message::Text(value))=> {
                        let Ok(packet)=serde_json::from_str::<Value>(&value) else {break;};
                        if packet["kind"]=="wsText" { Message::Text(packet["text"].as_str().unwrap_or_default().into()) }
                        else if packet["kind"]=="close" {
                            let code=packet["code"].as_u64().and_then(|v|u16::try_from(v).ok()).unwrap_or(1000);
                            let reason=packet["reason"].as_str().unwrap_or_default().to_string();
                            let _=socket.send(Message::Close(Some(axum::extract::ws::CloseFrame{code,reason:reason.into()}))).await;
                            break;
                        } else {break;}
                    },
                    _=>break,
                };
                if socket.send(message).await.is_err(){break;}
            }
        }
    }
    let _ = socket.close().await;
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/http_bridge.rs"]
mod tests;
