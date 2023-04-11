use super::JsonRpcClient;
use crate::rpc::{ApiClientError, ApiResult};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::de::Error;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::mpsc::{self, Sender, UnboundedReceiver, UnboundedSender};
// use tokio::sync::oneshot::{self, Sender};
use tokio_tungstenite as tokio_ws;
use tungstenite::protocol::Message;

pub struct TungsteniteClient {
    reqs: Arc<DashMap<u64, Sender<serde_json::Value>>>,
    to_back: UnboundedSender<serde_json::Value>,
    id: AtomicU64,
}

impl TungsteniteClient {
    pub fn new(url: &str) -> Self {
        let (to_back, from_front) = mpsc::unbounded_channel();
        let reqs = Arc::new(DashMap::new());
        let id = AtomicU64::new(1);
        Self::start_inner(url, from_front, reqs.clone());
        Self { reqs, to_back, id }
    }

    fn start_inner(
        url: &str,
        from_front: UnboundedReceiver<Value>,
        reqs: Arc<DashMap<u64, Sender<Value>>>,
    ) {
        let url = url.to_string();
        tokio::spawn(async move {
            let mut from_front = from_front;
            loop {
                let mut conn = match tokio_ws::connect_async(&url).await {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        log::error!("connect to {} failed, {:?}", url, e);
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        continue;
                    }
                };
                log::info!("connected to {}", url);
                let reqs = reqs.clone();
                from_front = tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            outgoing = from_front.recv() => {
                                log::debug!("==> outgoing msg: {:?}", outgoing);
                                match outgoing {
                                    Some(req) => {
                                        let payload = req.to_string();
                                        log::debug!("prepare sending request: {}", payload);
                                        let req = Message::Text(payload);
                                        if let Err(e) = conn.send(req).await {
                                            log::error!("sending request failed, {:?}", e);
                                            break;
                                        }
                                    }
                                    None => break,
                                }
                            }
                            incoming = conn.next() => {
                                log::debug!("<== incoming msg: {:?}", incoming);
                                match incoming {
                                    Some(Ok(msg)) => {
                                        match msg {
                                            Message::Text(msg) => {
                                                log::debug!("received response: {}", msg);
                                                let rsp: serde_json::Value = match serde_json::from_str(&msg) {
                                                    Ok(rsp) => rsp,
                                                    Err(e) => {
                                                        log::error!("deserializing response failed, {:?}", e);
                                                        continue;
                                                    }
                                                };
                                                let id = match rsp["id"].as_u64() {
                                                    Some(id) => id,
                                                    None => {
                                                        log::error!("response missing id field");
                                                        continue;
                                                    }
                                                };
                                                if let Some((_, tx)) = reqs.remove(&id) {
                                                    let _ = tx.send(rsp);
                                                }
                                            }
                                            Message::Ping(h) => {
                                                let _ = conn.send(Message::Pong(h)).await;
                                            }
                                            Message::Close(_) => {
                                                log::error!("connection closed by server");
                                                break;
                                            }
                                            _ => (),
                                        }
                                    }
                                    Some(Err(e)) => {
                                        log::error!("connection error, {:?}", e);
                                        break;
                                    }
                                    None => {
                                        log::error!("connection closed");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    from_front
                })
                .await
                .unwrap();
                log::error!("connection interrupted, retrying..");
            }
        });
    }
}

#[async_trait::async_trait]
impl JsonRpcClient for TungsteniteClient {
    async fn request(&self, mut req: serde_json::Value) -> ApiResult<Vec<u8>> {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = mpsc::channel(1);
        self.reqs.insert(id, tx);
        req["id"] = json!(id);
        self.to_back
            .send(req)
            .map_err(|_| ApiClientError::Timeout)?;
        tokio::select! {
            rsp = rx.recv() => {
                rsp.map(|v| serde_json::to_vec(&v["result"]).expect("-;qed"))
                   .ok_or(ApiClientError::Timeout)
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(100)) => {
                Err(ApiClientError::Timeout)
            }
        }
    }
}
