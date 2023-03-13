use ac_node_api::events::{EventsDecoder, Raw, RawEvent};
use codec::Decode;
use serde_json::Value;
use sp_core::Pair;
use sp_runtime::MultiSignature;
use std::sync::mpsc::{Receiver, SendError, Sender};

use crate::std::{
    json_req, rpc::RpcClientError, Api, ApiResult, FromHexString, RpcClient, XtStatus,
};
use crate::utils;

pub struct RpseeClient {}

// impl RpcClient for RpseeClient {
//     fn get_request(&self, jsonreq: serde_json::Value) -> ApiResult<String> {
//     }

//     fn send_extrinsic(&self, xthex_prefixed: String, exit_on: XtStatus) -> ApiResult<Option<Hash>> {
//     }
// }
