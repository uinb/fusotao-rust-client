use crate::{
    rpc::json_req,
    utils::ToHexString,
    {AccountId, Api, RpcClient},
};
use codec::{Decode, Encode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sp_core::crypto::Ss58Codec;
use sp_core::{sr25519::Pair as Sr25519Pair, Pair};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[derive(Eq, PartialEq, Clone, Encode, Decode, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TradingCommand {
    Ask {
        account_id: crate::AccountId,
        base: u32,
        quote: u32,
        amount: String,
        price: String,
    },
    Bid {
        account_id: crate::AccountId,
        base: u32,
        quote: u32,
        amount: String,
        price: String,
    },
    Cancel {
        account_id: crate::AccountId,
        base: u32,
        quote: u32,
        order_id: u64,
    },
}

impl TradingCommand {
    pub fn ask(
        account_id: crate::AccountId,
        base: u32,
        quote: u32,
        amount: Decimal,
        price: Decimal,
    ) -> Self {
        TradingCommand::Ask {
            account_id,
            base,
            quote,
            amount: amount.to_string(),
            price: price.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct Trader<Client>
where
    Client: RpcClient,
{
    api: Api<Sr25519Pair, Client>,
    // prover: AccounId,
    // empheral: x25519_dalek::EphemeralSecret,
    trading_key: [u8; 32],
    nonce: Arc<AtomicU32>,
}

#[derive(Clone, Encode, Decode, Eq, PartialEq)]
pub struct DominatorSetting {
    pub beneficiary: Option<AccountId>,
    pub x25519_pubkey: Vec<u8>,
    pub rpc_endpoint: Vec<u8>,
}

impl<Client> Trader<Client>
where
    Client: RpcClient,
{
    pub fn new(api: Api<Sr25519Pair, Client>, prover: AccountId) -> Self {
        let settings = api
            .get_storage_map::<AccountId, DominatorSetting>(
                "Verifier",
                "DominatorSettings",
                prover.clone(),
                None,
            )
            .expect("Failed to get prover x25519 pubkey")
            .expect("Prover not found");
        let pubkey: [u8; 32] = settings
            .x25519_pubkey
            .try_into()
            .expect("Invalid prover pubkey");
        let pubkey = x25519_dalek::PublicKey::from(pubkey);
        let empheral = x25519_dalek::EphemeralSecret::new(rand_core::OsRng);
        let trading_key = empheral.diffie_hellman(&pubkey).to_bytes();
        let signed = api
            .signer
            .as_ref()
            .expect("No signer")
            .sign(pubkey.as_bytes());
        let user_id = api
            .signer
            .as_ref()
            .expect("No signer")
            .public()
            .to_ss58check();
        let nonce = api.get_request(json_req::json_req(
            "broker_registerTradingKey",
            vec![
                prover.to_ss58check(),
                user_id,
                pubkey.to_bytes().to_hex(),
                signed.to_hex(),
            ],
            1,
        ));
        Self {
            api,
            trading_key,
            nonce: Arc::new(AtomicU32::new(0)),
        }
    }
}
