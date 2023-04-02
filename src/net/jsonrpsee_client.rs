use crate::primitives::Hash;
use crate::rpc::{ApiResult, XtStatus};
use dashmap::DashMap;
use jsonrpsee::core::{client::ClientT, Error as RpcError};
use jsonrpsee::types::params::Params;
use jsonrpsee::ws_client::WsClientBuilder;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender},
    oneshot::{self, Receiver, Sender},
    RwLock,
};

pub struct JsonrpseeClient {
    from_back: DashMap<u64, Receiver<serde_json::Value>>,
    to_back: UnboundedSender<serde_json::Value>,
    id: AtomicU64,
}

impl JsonrpseeClient {
    pub fn new(url: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self::start_inner(url, rx);
        Self {
            from_back: DashMap::new(),
            to_back: tx,
            id: AtomicU64::new(0),
        }
    }

    fn start_inner(url: String, mut rx: UnboundedReceiver<serde_json::Value>) {
        tokio::spawn(async move {
            let mut client = WsClientBuilder::default().build(&url).await.unwrap();
            loop {
                // TODO
                let to_be_sent = rx.recv().await;
                if to_be_sent.is_none() {
                    break;
                }
                let to_be_sent = to_be_sent.unwrap();
                if client.is_connected() {
                    let rsp: String = client
                        .request("", jsonrpsee::rpc_params! { to_be_sent.to_string() })
                        .await
                        .unwrap();
                } else {
                    match WsClientBuilder::default().build(&url).await {
                        Ok(c) => {
                            client = c;
                            tokio::spawn(async {
                                let rsp: String = client
                                    .request("", jsonrpsee::rpc_params! { to_be_sent.to_string() })
                                    .await
                                    .unwrap();
                            });
                        }
                        Err(e) => log::error!("failed to reconnect to {}, {:?}", url, e),
                    }
                }
            }
        });
    }
}

#[async_trait::async_trait]
impl crate::net::Client for JsonrpseeClient {
    fn request<T: Serialize>(&self, req: T) -> ApiResult<serde_json::Value> {
        // let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {});
        Ok(serde_json::Value::Null)
        // let req = serde_json::to_value(req).unwrap();
        // let req = serde_json::json!({
        //     "jsonrpc": "2.0",
        //     "method": "chain_getBlock",
        //     "params": [req],
        //     "id": id,
        // });
        // self.to_back.send(req).unwrap();
        // let res = rx.await.unwrap();
        // let res = res.as_object().unwrap();
        // let res = res.get("result").unwrap();
        // Ok(res.clone())
    }

    async fn request_async<T>(&self, req: T) -> ApiResult<serde_json::Value>
    where
        T: Serialize + Send,
    {
        Ok(serde_json::Value::Null)
    }
}
