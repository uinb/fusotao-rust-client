pub mod ws_client;

use crate::rpc::ApiResult;
use crate::rpc::XtStatus;
use sp_core::H256 as Hash;

pub trait RpcClient {
    /// Sends a RPC request that returns a String
    fn get_request(&self, jsonreq: serde_json::Value) -> ApiResult<String>;

    /// Send a RPC request that returns a SHA256 hash
    fn send_extrinsic(&self, xthex_prefixed: String, exit_on: XtStatus) -> ApiResult<Option<Hash>>;
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
