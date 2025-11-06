mod clients;
mod common;
mod subscribe_list;

use std::sync::{Arc};

use crate::state::PAIR_NAMES;
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
    for each in PAIR_NAMES.iter() {
        subscribe_list!(
            state,
            server,
            each,
            [BinanceWSClient, OkxWSClient, GateWSClient, BybitWSClient, WhitebitWSClient, BitgetWSClient, CryptoWSClient]
        )
    }
}
