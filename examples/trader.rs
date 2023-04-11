// #![feature(result_option_inspect)]
// use rust_decimal::Decimal;
use sp_keyring::AccountKeyring;
// use std::str::FromStr;
use substrate_api_client::{net::TungsteniteClient, Api};

// pub fn main() {
//     let client = WsRpcClient::new("ws://127.0.0.1:9944");
//     let mut api = Api::<sp_core::sr25519::Pair, WsRpcClient>::new(client).unwrap();
//     api = api.set_signer(AccountKeyring::Bob.pair());
//     let trader = Trader::new(api, AccountKeyring::Alice.to_account_id()).unwrap();
//     println!("{:?}", trader.query_orders(0, 1));
//     println!("{:?}", trader.query_account());
//     trader.ask(
//         0,
//         1,
//         Decimal::from_str("1").unwrap(),
//         Decimal::from_str("9").unwrap(),
//     );
//     trader.cancel(0, 1, 2);
// }
#[tokio::main]
pub async fn main() {
    env_logger::init();
    let client = TungsteniteClient::new(
        "wss://gateway.mainnet.octopus.network/fusotao/0efwa9v0crdx4dg3uj8jdmc5y7dj4ir2",
    );
    let api = Api::<sp_core::sr25519::Pair, TungsteniteClient>::new(
        client,
        Some(AccountKeyring::Bob.pair()),
    )
    .await
    .unwrap();
}
