//pub mod jsonrpsee_client;
pub mod ws_client;

use crate::primitives::Hash;
use crate::rpc::{ApiResult, XtStatus};
use serde::Serialize;

pub use ws_client::WsRpcClient;

pub trait RpcClient {
    /// Sends a RPC request that returns a String
    fn get_request(&self, jsonreq: serde_json::Value) -> ApiResult<String>;

    /// Send a RPC request that returns a BlakeTwo_256 hash
    fn send_extrinsic(&self, xthex_prefixed: String, exit_on: XtStatus) -> ApiResult<Option<Hash>>;
}

#[async_trait::async_trait]
pub trait Client {
    fn request<T: Serialize>(&self, req: T) -> ApiResult<serde_json::Value>;

    async fn request_async<T>(&self, req: T) -> ApiResult<serde_json::Value>
    where
        T: Serialize + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum RpcClientError {
    #[error("Serde json error: {0}")]
    Serde(#[from] serde_json::error::Error),
    #[error("Extrinsic Error: {0}")]
    Extrinsic(String),
    #[error("mpsc send Error: {0}")]
    Send(#[from] std::sync::mpsc::SendError<String>),
}
