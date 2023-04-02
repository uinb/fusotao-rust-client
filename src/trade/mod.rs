use crate::{primitives::AccountId, rpc::json_req, Api, DecodeHexString, RpcClient, ToHexString};
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
        account_id: AccountId,
        base: u32,
        quote: u32,
        amount: String,
        price: String,
    },
    Bid {
        account_id: AccountId,
        base: u32,
        quote: u32,
        amount: String,
        price: String,
    },
    Cancel {
        account_id: AccountId,
        base: u32,
        quote: u32,
        order_id: u64,
    },
}

impl TradingCommand {
    pub fn ask(
        account_id: AccountId,
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
    prover: AccountId,
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
    pub fn new(api: Api<Sr25519Pair, Client>, prover: AccountId) -> Option<Self> {
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
        let prover_pubkey = x25519_dalek::PublicKey::from(pubkey);
        let ephemeral = x25519_dalek::EphemeralSecret::new(rand_core::OsRng);
        let self_pubkey = x25519_dalek::PublicKey::from(&ephemeral).to_bytes();
        let trading_key = ephemeral.diffie_hellman(&prover_pubkey).to_bytes();
        let signed = api.signer.as_ref().expect("No signer").sign(&self_pubkey);
        let user_id = api
            .signer
            .as_ref()
            .expect("No signer")
            .public()
            .to_ss58check();
        let req = json_req::json_req(
            "broker_registerTradingKey",
            vec![
                prover.to_ss58check(),
                user_id,
                self_pubkey.to_hex(),
                signed.to_hex(),
            ],
            1,
        );
        let nonce = api
            .get_request(req)
            .inspect_err(|e| log::error!("Failed to register trading key: {}", e))
            .ok()
            .flatten()?;
        let nonce = u32::decode_hex(nonce).ok()?;
        Some(Self {
            api,
            prover,
            trading_key,
            nonce: Arc::new(AtomicU32::new(nonce)),
        })
    }

    pub fn sync_nonce(&self) {
        let nonce = self
            .api
            .get_request(json_req::json_req(
                "broker_getNonce",
                vec![self.api.signer.as_ref().unwrap().public().to_ss58check()],
                1,
            ))
            .expect("Failed to get nonce")
            .unwrap();
        let nonce = u32::decode_hex(nonce).expect("Invalid nonce");
        self.nonce.store(nonce, Ordering::Relaxed);
    }

    pub fn query_orders(&self, base: u32, quote: u32) -> Vec<String> {
        let symbol = (base, quote).encode();
        let (sig, nonce) = self.sign_req(&symbol);
        let req = json_req::json_req(
            "broker_queryPendingOrders",
            vec![
                self.prover.to_ss58check(),
                format!("0x{}", hex::encode(symbol)),
                self.api.signer.as_ref().unwrap().public().to_ss58check(),
                sig,
                nonce,
            ],
            1,
        );
        let r = self
            .api
            .get_request(req)
            .expect("Failed to get orders")
            .unwrap();
        println!("r: {}", r);
        vec![]
    }

    pub fn query_account(&self) -> Vec<String> {
        let (sig, nonce) = self.sign_req(&[]);
        let req = json_req::json_req(
            "broker_queryAccount",
            vec![
                self.prover.to_ss58check(),
                self.api.signer.as_ref().unwrap().public().to_ss58check(),
                sig,
                nonce,
            ],
            1,
        );
        let r = self
            .api
            .get_request(req)
            .expect("Failed to get account")
            .unwrap();
        let r: Vec<String> = serde_json::from_str(&r).expect("Invalid account");
        for a in r.into_iter() {
            println!("account: {:?}", <(u32, (String, String))>::decode_hex(a));
        }
        vec![]
    }

    pub fn ask(&self, base: u32, quote: u32, amount: Decimal, price: Decimal) -> Option<String> {
        let cmd = TradingCommand::ask(
            self.api.signer.as_ref().unwrap().public().into(),
            base,
            quote,
            amount,
            price,
        );
        self.trade(cmd)
    }

    pub fn cancel(&self, base: u32, quote: u32, order_id: u64) -> Option<String> {
        let cmd = TradingCommand::Cancel {
            account_id: self.api.signer.as_ref().unwrap().public().into(),
            base,
            quote,
            order_id,
        };
        self.trade(cmd)
    }

    fn trade(&self, cmd: TradingCommand) -> Option<String> {
        let cmd = cmd.encode();
        let (sig, nonce) = self.sign_req(&cmd);
        let req = json_req::json_req(
            "broker_trade",
            vec![
                self.prover.to_ss58check(),
                format!("0x{}", hex::encode(cmd)),
                sig,
                nonce,
            ],
            1,
        );
        let r = self
            .api
            .get_request(req)
            .expect("Failed to place order")
            .unwrap();
        println!("order ---> {}", r.to_string());
        None
    }

    fn sign_req(&self, payload: &[u8]) -> (String, String) {
        let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
        let nonce = nonce.encode();
        let mut to_be_signed = vec![];
        to_be_signed.extend_from_slice(payload);
        to_be_signed.extend_from_slice(self.trading_key.as_slice());
        to_be_signed.extend_from_slice(nonce.as_slice());
        let signed = sp_core::blake2_256(&to_be_signed);
        (
            format!("0x{}", hex::encode(signed)),
            format!("0x{}", hex::encode(nonce)),
        )
    }
}
