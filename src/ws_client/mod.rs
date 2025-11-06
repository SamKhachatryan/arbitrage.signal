mod clients;
mod common;
mod subscribe_list;

use std::sync::{Arc};

// BEGIN FOR MACRO
use crate::subscribe_list;
use crate::ws_server::WSServer;
use crate::{
    state::AppState,
    ws_client::{
        clients::{
            binance::BinanceWSClient,
            bybit::BybitWSClient,
            gate::GateWSClient,
            okx::OkxWSClient,
            whitebit::WhitebitWSClient,
            bitget::BitgetWSClient,
            crypto::CryptoWSClient,
        },
        common::ExchangeWSClient,
    },
};
use futures_util::FutureExt;
use tokio::sync::Mutex;
// END FOR MACRO

pub async fn subscribe_to_all_exchanges(
    state: &Arc<Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
) {
    let pair_names = vec![
        "btc-usdt",
        "eth-usdt",
        "sol-usdt",
        "doge-usdt",
        "xrp-usdt",
        "ton-usdt",
        "ada-usdt",
        "link-usdt",
        "arb-usdt",
        "op-usdt",
        "ltc-usdt",
        "bch-usdt",
        "uni-usdt",
        "avax-usdt",
        "apt-usdt",
        "near-usdt",
        "matic-usdt",
        "pepe-usdt",
        "floki-usdt",
        "sui-usdt",
    ];

    for each in pair_names {
        subscribe_list!(
            state,
            server,
            each,
            [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient, WhitebitWSClient, BitgetWSClient, CryptoWSClient]
        )
    }
}
