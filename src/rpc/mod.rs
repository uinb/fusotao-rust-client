pub mod error;
pub mod json_req;

pub use error::{ApiResult, Error as ApiClientError};

use crate::primitives::{AccountData, AccountInfo, Balance, Hash};
use crate::rpc;
use crate::runtime_types::metadata::{Metadata, MetadataError};
use crate::{net::JsonRpcClient, FromHexString};
use codec::{Decode, Encode};
use frame_metadata::RuntimeMetadataPrefixed;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sp_core::{crypto::Pair, storage::StorageKey};
use sp_runtime::{
    generic::SignedBlock,
    traits::{Block, Header, IdentifyAccount},
    AccountId32 as AccountId, MultiSignature, MultiSigner,
};
use sp_version::RuntimeVersion;
use std::convert::TryFrom;

pub type StoragePair = (Vec<u8>, Vec<u8>);

#[derive(Clone)]
pub struct Api<P, Client>
where
    Client: JsonRpcClient,
{
    pub signer: Option<P>,
    pub metadata: Metadata,
    pub client: Client,
}

impl<P, Client> Api<P, Client>
where
    P: Pair,
    MultiSignature: From<P::Signature>,
    MultiSigner: From<P::Public>,
    Client: JsonRpcClient,
{
    pub fn signer(&self) -> Option<AccountId> {
        let pair = self.signer.as_ref()?;
        let multi_signer = MultiSigner::from(pair.public());
        Some(multi_signer.into_account())
    }
}

impl<P, Client> Api<P, Client>
where
    Client: JsonRpcClient,
{
    pub async fn new(client: Client, signer: Option<P>) -> ApiResult<Self> {
        let metadata = client
            .request(rpc!(state, getMetadata, []))
            .await
            .map(|b| RuntimeMetadataPrefixed::decode(&mut b.as_slice()))?
            .map(Metadata::try_from)??;
        log::debug!("Metadata: {:?}", metadata);
        Ok(Self {
            signer,
            metadata,
            client,
        })
    }

    pub fn get_storage_map_key_prefix(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
    ) -> ApiResult<StorageKey> {
        self.metadata
            .storage_map_key_prefix(storage_prefix, storage_key_name)
            .map_err(|e| e.into())
    }

    pub fn get_storage_double_map_partial_prefix<K: Encode>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        first: &K,
    ) -> ApiResult<StorageKey> {
        self.metadata
            .storage_double_map_partial_key(storage_prefix, storage_key_name, first)
            .map_err(|e| e.into())
    }

    // TODO
    // pub fn get_opaque_storage_pairs_by_key_hash(
    //     &self,
    //     key: StorageKey,
    //     at_block: Option<Hash>,
    // ) -> ApiResult<Option<Vec<StoragePair>>> {
    //     let jsonreq = json_req::state_get_pairs(key, at_block);
    //     let s = self.get_request(jsonreq)?;
    //     match s {
    //         Some(storage) => {
    //             let deser: Vec<(String, String)> = serde_json::from_str(&storage)?;
    //             let mut kv = vec![];
    //             for (k, v) in deser.into_iter() {
    //                 kv.push((Vec::from_hex(k)?, Vec::from_hex(v)?));
    //             }
    //             Ok(Some(kv))
    //         }
    //         None => Ok(None),
    //     }
    // }

    pub fn get_constant<C: Decode>(
        &self,
        pallet: &'static str,
        constant: &'static str,
    ) -> ApiResult<C> {
        let c = self
            .metadata
            .pallet(pallet)?
            .constants
            .get(constant)
            .ok_or(MetadataError::ConstantNotFound(constant))?;
        Ok(Decode::decode(&mut c.value.as_slice())?)
    }

    // pub fn send_extrinsic(
    //     &self,
    //     xthex_prefixed: String,
    //     exit_on: XtStatus,
    // ) -> ApiResult<Option<Hash>> {
    //     log::debug!("sending extrinsic: {:?}", xthex_prefixed);
    //     self.client.send_extrinsic(xthex_prefixed, exit_on)
    // }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum XtStatus {
    Finalized,
    InBlock,
    Broadcast,
    Ready,
    Future,
    SubmitOnly,
    Error,
    Unknown,
}

pub fn storage_prefix(module: &str, storage: &str) -> Vec<u8> {
    let mut key = sp_core::twox_128(module.as_bytes()).to_vec();
    key.extend(&sp_core::twox_128(storage.as_bytes()));
    key
}
