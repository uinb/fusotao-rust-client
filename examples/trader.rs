#![feature(result_option_inspect)]
use rust_decimal::Decimal;
use sp_keyring::AccountKeyring;
use std::str::FromStr;
use substrate_api_client::{net::WsRpcClient, trade::Trader, Api};

pub fn main() {
    let client = WsRpcClient::new("ws://127.0.0.1:9944");
    let mut api = Api::<sp_core::sr25519::Pair, WsRpcClient>::new(client).unwrap();
    api = api.set_signer(AccountKeyring::Bob.pair());
    let trader = Trader::new(api, AccountKeyring::Alice.to_account_id()).unwrap();
    println!("{:?}", trader.query_orders(0, 1));
    println!("{:?}", trader.query_account());
    trader.ask(
        0,
        1,
        Decimal::from_str("1").unwrap(),
        Decimal::from_str("9").unwrap(),
    );
    trader.cancel(0, 1, 2);
}
