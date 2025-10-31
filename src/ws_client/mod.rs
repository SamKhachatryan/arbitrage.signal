mod common;
mod clients;
mod subscribe_list;

use std::sync::{Arc, Mutex};

// BEGIN FOR MACRO
use crate::subscribe_list;
use crate::ws_server::WSServer;
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

pub async fn subscribe_to_all_exchanges(state: &Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>) {
    subscribe_list!(
        state,
        server,
        "btc-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        server,
        "eth-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        server,
        "xrp-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        server,
        "doge-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        server,
        "ton-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );

    subscribe_list!(
        state,
        server,
        "sol-usdt",
        [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient]
    );
}
