use serde_json::Value;
use sp_core::H256 as Hash;
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender;
use ws::Result as WsResult;

// use crate::net::ws_client::{
//     on_extrinsic_msg_submit_only, on_extrinsic_msg_until_broadcast,
//     on_extrinsic_msg_until_finalized, on_extrinsic_msg_until_in_block,
//     on_extrinsic_msg_until_ready, on_get_request_msg, OnMessageFn, RpcClient,
// };
use crate::net::RpcClient as RpcClientTrait;
use crate::rpc::json_req;
use crate::rpc::ApiClientError;
use crate::rpc::ApiResult;
use crate::rpc::XtStatus;
use crate::FromHexString;

#[derive(Debug, Clone)]
pub struct WsRpcClient {
    url: String,
}

pub struct Session {
    pub sink: ws::Sender,
    pub result_in: ThreadOut<String>,
}

impl Handler for Session {
    fn on_open(&mut self, _: Handshake) -> WsResult<()> {
        info!("sending request: {}", self.request);
        self.out.send(self.request.clone())?;
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        (self.on_message_fn)(msg, self.out.clone(), self.result.clone())
    }
}

impl WsRpcClient {
    pub fn new(url: &str) -> WsRpcClient {
        WsRpcClient {
            url: url.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl JsonRpcClient for WsRpcClient {
    async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> ApiResult<serde_json::Value> {
    }
}

impl RpcClientTrait for WsRpcClient {
    fn get_request(&self, jsonreq: Value) -> ApiResult<String> {
        let (result_in, result_out) = channel();
        self.get(jsonreq.to_string(), result_in)?;

        let str = result_out.recv()?;
        Ok(str)
    }

    fn send_extrinsic(
        &self,
        xthex_prefixed: String,
        exit_on: XtStatus,
    ) -> ApiResult<Option<sp_core::H256>> {
        // Todo: Make all variants return a H256: #175.

        let jsonreq = match exit_on {
            XtStatus::SubmitOnly => json_req::author_submit_extrinsic(&xthex_prefixed).to_string(),
            _ => json_req::author_submit_and_watch_extrinsic(&xthex_prefixed).to_string(),
        };

        let (result_in, result_out) = channel();
        match exit_on {
            XtStatus::Finalized => {
                self.send_extrinsic_and_wait_until_finalized(jsonreq, result_in)?;
                let res = result_out.recv()?;
                info!("finalized: {}", res);
                Ok(Some(Hash::from_hex(res)?))
            }
            XtStatus::InBlock => {
                self.send_extrinsic_and_wait_until_in_block(jsonreq, result_in)?;
                let res = result_out.recv()?;
                info!("inBlock: {}", res);
                Ok(Some(Hash::from_hex(res)?))
            }
            XtStatus::Broadcast => {
                self.send_extrinsic_and_wait_until_broadcast(jsonreq, result_in)?;
                let res = result_out.recv()?;
                info!("broadcast: {}", res);
                Ok(None)
            }
            XtStatus::Ready => {
                self.send_extrinsic_until_ready(jsonreq, result_in)?;
                let res = result_out.recv()?;
                info!("ready: {}", res);
                Ok(None)
            }
            XtStatus::SubmitOnly => {
                self.send_extrinsic(jsonreq, result_in)?;
                let res = result_out.recv()?;
                info!("submitted xt: {}", res);
                Ok(None)
            }
            _ => Err(ApiClientError::UnsupportedXtStatus(exit_on)),
        }
    }
}

impl WsRpcClient {
    pub fn get(&self, json_req: String, result_in: ThreadOut<String>) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_get_request_msg)
    }

    pub fn send_extrinsic(&self, json_req: String, result_in: ThreadOut<String>) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_extrinsic_msg_submit_only)
    }

    pub fn send_extrinsic_until_ready(
        &self,
        json_req: String,
        result_in: ThreadOut<String>,
    ) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_extrinsic_msg_until_ready)
    }

    pub fn send_extrinsic_and_wait_until_broadcast(
        &self,
        json_req: String,
        result_in: ThreadOut<String>,
    ) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_extrinsic_msg_until_broadcast)
    }

    pub fn send_extrinsic_and_wait_until_in_block(
        &self,
        json_req: String,
        result_in: ThreadOut<String>,
    ) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_extrinsic_msg_until_in_block)
    }

    pub fn send_extrinsic_and_wait_until_finalized(
        &self,
        json_req: String,
        result_in: ThreadOut<String>,
    ) -> WsResult<()> {
        self.start_rpc_client_thread(json_req, result_in, on_extrinsic_msg_until_finalized)
    }

    fn start_rpc_client_thread(
        &self,
        jsonreq: String,
        result_in: ThreadOut<String>,
        on_message_fn: OnMessageFn,
    ) -> WsResult<()> {
        let url = self.url.clone();
        let _client =
            thread::Builder::new()
                .name("client".to_owned())
                .spawn(move || -> WsResult<()> {
                    ws::connect(url, |out| RpcClient {
                        out,
                        request: jsonreq.clone(),
                        result: result_in.clone(),
                        on_message_fn,
                    })
                })?;
        Ok(())
    }
}
