pub mod tungstenite_client;

use crate::{
    primitives::Hash,
    rpc::{ApiResult, XtStatus},
};
pub use tungstenite_client::TungsteniteClient;

// TODO deprecated
pub trait RpcClient {
    /// Sends a RPC request that returns a String
    fn get_request(&self, jsonreq: serde_json::Value) -> ApiResult<String>;

    /// Send a RPC request that returns a BlakeTwo_256 hash
    fn send_extrinsic(&self, xthex_prefixed: String, exit_on: XtStatus) -> ApiResult<Option<Hash>>;
}

#[async_trait::async_trait]
pub trait JsonRpcClient {
    async fn request(&self, req: serde_json::Value) -> ApiResult<Vec<u8>>;
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
