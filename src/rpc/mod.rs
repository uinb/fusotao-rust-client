pub mod error;
pub mod json_req;

pub use error::{ApiResult, Error as ApiClientError};

use crate::primitives::{AccountData, AccountInfo, Balance, Hash};
use crate::runtime_types::metadata::{Metadata, MetadataError};
use crate::{FromHexString, RpcClient};
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
    Client: RpcClient,
{
    pub signer: Option<P>,
    pub genesis_hash: Hash,
    pub metadata: Metadata,
    pub runtime_version: RuntimeVersion,
    pub client: Client,
}

impl<P, Client> Api<P, Client>
where
    P: Pair,
    MultiSignature: From<P::Signature>,
    MultiSigner: From<P::Public>,
    Client: RpcClient,
{
    pub fn signer_account(&self) -> Option<AccountId> {
        let pair = self.signer.as_ref()?;
        let multi_signer = MultiSigner::from(pair.public());
        Some(multi_signer.into_account())
    }

    pub fn get_nonce(&self) -> ApiResult<u32> {
        if self.signer.is_none() {
            return Err(ApiClientError::NoSigner);
        }

        self.get_account_info(&self.signer_account().unwrap())
            .map(|acc_opt| acc_opt.map_or_else(|| 0, |acc| acc.nonce))
    }
}

impl<P, Client> Api<P, Client>
where
    Client: RpcClient,
{
    pub fn new(client: Client) -> ApiResult<Self> {
        let genesis_hash = Self::_get_genesis_hash(&client)?;
        log::debug!("Got genesis hash: {:?}", genesis_hash);

        let metadata = Self::_get_metadata(&client).map(Metadata::try_from)??;
        log::debug!("Metadata: {:?}", metadata);

        let runtime_version = Self::_get_runtime_version(&client)?;
        log::debug!("Runtime Version: {:?}", runtime_version);

        Ok(Self {
            signer: None,
            genesis_hash,
            metadata,
            runtime_version,
            client,
        })
    }

    #[must_use]
    pub fn set_signer(mut self, signer: P) -> Self {
        self.signer = Some(signer);
        self
    }

    fn _get_genesis_hash(client: &Client) -> ApiResult<Hash> {
        let jsonreq = json_req::chain_get_genesis_hash();
        let genesis = Self::_get_request(client, jsonreq)?;

        match genesis {
            Some(g) => Hash::from_hex(g).map_err(|e| e.into()),
            None => Err(ApiClientError::Genesis),
        }
    }

    fn _get_runtime_version(client: &Client) -> ApiResult<RuntimeVersion> {
        let jsonreq = json_req::state_get_runtime_version();
        let version = Self::_get_request(client, jsonreq)?;

        match version {
            Some(v) => serde_json::from_str(&v).map_err(|e| e.into()),
            None => Err(ApiClientError::RuntimeVersion),
        }
    }

    fn _get_metadata(client: &Client) -> ApiResult<RuntimeMetadataPrefixed> {
        let jsonreq = json_req::state_get_metadata();
        let meta = Self::_get_request(client, jsonreq)?;

        if meta.is_none() {
            return Err(ApiClientError::MetadataFetch);
        }
        let metadata = Vec::from_hex(meta.unwrap())?;
        RuntimeMetadataPrefixed::decode(&mut metadata.as_slice()).map_err(|e| e.into())
    }

    // low level access
    fn _get_request(client: &Client, jsonreq: Value) -> ApiResult<Option<String>> {
        let str = client.get_request(jsonreq)?;

        match &str[..] {
            "null" => Ok(None),
            _ => Ok(Some(str)),
        }
    }

    pub fn get_metadata(&self) -> ApiResult<RuntimeMetadataPrefixed> {
        Self::_get_metadata(&self.client)
    }

    pub fn get_spec_version(&self) -> ApiResult<u32> {
        Self::_get_runtime_version(&self.client).map(|v| v.spec_version)
    }

    pub fn get_genesis_hash(&self) -> ApiResult<Hash> {
        Self::_get_genesis_hash(&self.client)
    }

    pub fn get_account_info(&self, address: &AccountId) -> ApiResult<Option<AccountInfo>> {
        let storagekey: sp_core::storage::StorageKey =
            self.metadata
                .storage_map_key::<AccountId>("System", "Account", address.clone())?;

        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_by_key_hash(storagekey, None)
    }

    pub fn get_account_data(&self, address: &AccountId) -> ApiResult<Option<AccountData>> {
        self.get_account_info(address)
            .map(|info| info.map(|i| i.data))
    }

    pub fn get_finalized_head(&self) -> ApiResult<Option<Hash>> {
        let h = self.get_request(json_req::chain_get_finalized_head())?;
        match h {
            Some(hash) => Ok(Some(Hash::from_hex(hash)?)),
            None => Ok(None),
        }
    }

    pub fn get_header<H>(&self, hash: Option<Hash>) -> ApiResult<Option<H>>
    where
        H: Header + DeserializeOwned,
    {
        let h = self.get_request(json_req::chain_get_header(hash))?;
        match h {
            Some(hash) => Ok(Some(serde_json::from_str(&hash)?)),
            None => Ok(None),
        }
    }

    pub fn get_block_hash(&self, number: Option<u32>) -> ApiResult<Option<Hash>> {
        let h = self.get_request(json_req::chain_get_block_hash(number))?;
        match h {
            Some(hash) => Ok(Some(Hash::from_hex(hash)?)),
            None => Ok(None),
        }
    }

    pub fn get_block<B>(&self, hash: Option<Hash>) -> ApiResult<Option<B>>
    where
        B: Block + DeserializeOwned,
    {
        Self::get_signed_block(self, hash).map(|sb_opt| sb_opt.map(|sb| sb.block))
    }

    pub fn get_block_by_num<B>(&self, number: Option<u32>) -> ApiResult<Option<B>>
    where
        B: Block + DeserializeOwned,
    {
        Self::get_signed_block_by_num(self, number).map(|sb_opt| sb_opt.map(|sb| sb.block))
    }

    /// A signed block is a block with Justification ,i.e., a Grandpa finality proof.
    /// The interval at which finality proofs are provided is set via the
    /// the `GrandpaConfig.justification_period` in a node's service.rs.
    /// The Justification may be none.
    pub fn get_signed_block<B>(&self, hash: Option<Hash>) -> ApiResult<Option<SignedBlock<B>>>
    where
        B: Block + DeserializeOwned,
    {
        let b = self.get_request(json_req::chain_get_block(hash))?;
        match b {
            Some(block) => Ok(Some(serde_json::from_str(&block)?)),
            None => Ok(None),
        }
    }

    pub fn get_signed_block_by_num<B>(
        &self,
        number: Option<u32>,
    ) -> ApiResult<Option<SignedBlock<B>>>
    where
        B: Block + DeserializeOwned,
    {
        self.get_block_hash(number)
            .map(|h| self.get_signed_block(h))?
    }

    pub fn get_request(&self, jsonreq: Value) -> ApiResult<Option<String>> {
        Self::_get_request(&self.client, jsonreq)
    }

    pub fn get_storage_value<V: Decode>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<V>> {
        let storagekey = self
            .metadata
            .storage_value_key(storage_prefix, storage_key_name)?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_by_key_hash(storagekey, at_block)
    }

    pub fn get_storage_map<K: Encode, V: Decode + Clone>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        map_key: K,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<V>> {
        let storagekey =
            self.metadata
                .storage_map_key::<K>(storage_prefix, storage_key_name, map_key)?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_by_key_hash(storagekey, at_block)
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

    pub fn get_storage_double_map<K: Encode, Q: Encode, V: Decode + Clone>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        first: K,
        second: Q,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<V>> {
        let storagekey = self.metadata.storage_double_map_key::<K, Q>(
            storage_prefix,
            storage_key_name,
            first,
            second,
        )?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_by_key_hash(storagekey, at_block)
    }

    pub fn get_storage_by_key_hash<V: Decode>(
        &self,
        key: StorageKey,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<V>> {
        let s = self.get_opaque_storage_by_key_hash(key, at_block)?;
        match s {
            Some(storage) => Ok(Some(Decode::decode(&mut storage.as_slice())?)),
            None => Ok(None),
        }
    }

    pub fn get_opaque_storage_by_key_hash(
        &self,
        key: StorageKey,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<Vec<u8>>> {
        let jsonreq = json_req::state_get_storage(key, at_block);
        let s = self.get_request(jsonreq)?;

        match s {
            Some(storage) => Ok(Some(Vec::from_hex(storage)?)),
            None => Ok(None),
        }
    }

    // TODO
    pub fn get_opaque_storage_pairs_by_key_hash(
        &self,
        key: StorageKey,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<Vec<StoragePair>>> {
        let jsonreq = json_req::state_get_pairs(key, at_block);
        let s = self.get_request(jsonreq)?;
        match s {
            Some(storage) => {
                let deser: Vec<(String, String)> = serde_json::from_str(&storage)?;
                let mut kv = vec![];
                for (k, v) in deser.into_iter() {
                    kv.push((Vec::from_hex(k)?, Vec::from_hex(v)?));
                }
                Ok(Some(kv))
            }
            None => Ok(None),
        }
    }

    pub fn get_storage_value_proof(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<ReadProof<Hash>>> {
        let storagekey = self
            .metadata
            .storage_value_key(storage_prefix, storage_key_name)?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_proof_by_keys(vec![storagekey], at_block)
    }

    pub fn get_storage_map_proof<K: Encode, V: Decode + Clone>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        map_key: K,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<ReadProof<Hash>>> {
        let storagekey =
            self.metadata
                .storage_map_key::<K>(storage_prefix, storage_key_name, map_key)?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_proof_by_keys(vec![storagekey], at_block)
    }

    pub fn get_storage_double_map_proof<K: Encode, Q: Encode, V: Decode + Clone>(
        &self,
        storage_prefix: &'static str,
        storage_key_name: &'static str,
        first: K,
        second: Q,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<ReadProof<Hash>>> {
        let storagekey = self.metadata.storage_double_map_key::<K, Q>(
            storage_prefix,
            storage_key_name,
            first,
            second,
        )?;
        log::debug!("storage key is: 0x{}", hex::encode(&storagekey));
        self.get_storage_proof_by_keys(vec![storagekey], at_block)
    }

    pub fn get_storage_proof_by_keys(
        &self,
        keys: Vec<StorageKey>,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<ReadProof<Hash>>> {
        let jsonreq = json_req::state_get_read_proof(keys, at_block);
        let p = self.get_request(jsonreq)?;
        match p {
            Some(proof) => Ok(Some(serde_json::from_str(&proof)?)),
            None => Ok(None),
        }
    }

    pub fn get_keys(
        &self,
        key: StorageKey,
        at_block: Option<Hash>,
    ) -> ApiResult<Option<Vec<Vec<u8>>>> {
        let jsonreq = json_req::state_get_keys(key, at_block);
        let k = self.get_request(jsonreq)?;
        match k {
            Some(keys) => {
                let deser: Vec<String> = serde_json::from_str(&keys)?;
                let mut keys = vec![];
                for k in deser.into_iter() {
                    keys.push(Vec::from_hex(k)?);
                }
                Ok(Some(keys))
            }
            None => Ok(None),
        }
    }

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

    pub fn get_existential_deposit(&self) -> ApiResult<Balance> {
        self.get_constant("Balances", "ExistentialDeposit")
    }

    pub fn send_extrinsic(
        &self,
        xthex_prefixed: String,
        exit_on: XtStatus,
    ) -> ApiResult<Option<Hash>> {
        log::debug!("sending extrinsic: {:?}", xthex_prefixed);
        self.client.send_extrinsic(xthex_prefixed, exit_on)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum XtStatus {
    // Todo: some variants to not return a hash with `send_extrinsics`: #175.
    Finalized,
    InBlock,
    Broadcast,
    Ready,
    Future,
    /// uses `author_submit`
    SubmitOnly,
    Error,
    Unknown,
}

// Exact structure from
// https://github.com/paritytech/substrate/blob/master/client/rpc-api/src/state/helpers.rs
// Adding manually so we don't need sc-rpc-api, which brings in async dependencies
#[derive(Debug, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
pub struct ReadProof<Hash> {
    /// Block hash used to generate the proof
    pub at: Hash,
    /// A proof used to prove that storage entries are included in the storage trie
    pub proof: Vec<sp_core::Bytes>,
}

pub fn storage_key(module: &str, storage_key_name: &str) -> StorageKey {
    let mut key = sp_core::twox_128(module.as_bytes()).to_vec();
    key.extend(&sp_core::twox_128(storage_key_name.as_bytes()));
    StorageKey(key)
}
