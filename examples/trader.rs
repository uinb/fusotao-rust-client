use sp_keyring::AccountKeyring;
use substrate_api_client::{rpc::WsRpcClient, trade::Trader, Api};

pub fn main() {
    let client = WsRpcClient::new("ws://127.0.0.1:9944");
    let mut api = Api::<sp_core::sr25519::Pair, WsRpcClient>::new(client).unwrap();
    api = api.set_signer(AccountKeyring::Alice.pair());
    let trader = Trader::new(api, AccountKeyring::Alice.to_account_id());
}
