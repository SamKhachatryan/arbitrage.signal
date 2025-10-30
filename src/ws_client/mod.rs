mod clients;
mod common;
mod subscribe_list;

use std::sync::{Arc, Mutex};

// BEGIN FOR MACRO
use crate::subscribe_list;
use crate::{
    state::AppState,
    ws_client::{
        clients::{
            binance::BinanceWSClient, bybit::BybitWSClient, gate::GateWSClient, okx::OkxWSClient,
        },
        common::ExchangeWSClient,
    },
};
use futures_util::FutureExt;
// END FOR MACRO

pub async fn subscribe_to_all_exchanges(state: &Arc<Mutex<AppState>>) {
    subscribe_list!(
        state,
        "btc-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        "eth-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        "xrp-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        "doge-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        "ton-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        "sol-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );
}
