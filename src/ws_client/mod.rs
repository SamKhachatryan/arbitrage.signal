mod clients;
mod common;
mod subscribe_list;

use std::sync::Arc;

use crate::state::PAIR_NAMES;
// BEGIN FOR MACRO
use crate::subscribe_list;
use crate::ws_server::WSServer;
use crate::{
    state::AppState,
    ws_client::clients::{
        binance::BinanceExchangeWSSession,
        bitget::BitgetExchangeWSSession,
        bybit::BybitExchangeWSSession,
        // crypto::CryptoExchangeWSSession,
        gate::GateExchangeWSSession,
        interface::{ExchangeWSClient, WSClient},
        okx::OkxExchangeWSSession,
        whitebit::WhitebitExchangeWSSession,
    },
};
// END FOR MACRO

pub async fn subscribe_to_all_exchanges(
    state: &Arc<std::sync::Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
) {
    // let cloned_state = Arc::clone(&state);
    // let cloned_server = Arc::clone(&server);

    // let client = WSClient::new(
    //     CryptoExchangeWSSession {},
    //     std::env::var("CRYPTO_WS_URL").expect("CRYPTO_WS_URL failed"),
    // );

    // client
    //     .subscribe(
    //         cloned_state,
    //         cloned_server,
    //         PAIR_NAMES[0..10].iter().map(|s| s.to_string()).collect(),
    //     )
    //     .await;

    // let cloned_state = Arc::clone(&state);
    // let cloned_server = Arc::clone(&server);

    // let client = WSClient::new(
    //     CryptoExchangeWSSession {},
    //     std::env::var("CRYPTO_WS_URL").expect("CRYPTO_WS_URL failed"),
    // );

    // client
    //     .subscribe(
    //         cloned_state,
    //         cloned_server,
    //         PAIR_NAMES[10..20].iter().map(|s| s.to_string()).collect(),
    //     )
    //     .await;

    // let cloned_state = Arc::clone(&state);
    // let cloned_server = Arc::clone(&server);

    // let client = WSClient::new(
    //     CryptoExchangeWSSession {},
    //     std::env::var("CRYPTO_WS_URL").expect("CRYPTO_WS_URL failed"),
    // );

    // client
    //     .subscribe(
    //         cloned_state,
    //         cloned_server,
    //         PAIR_NAMES[20..30].iter().map(|s| s.to_string()).collect(),
    //     )
    //     .await;

    for each in PAIR_NAMES.iter() {
        let binance_pair_url = format!(
            "{}/{}@trade",
            std::env::var("BINANCE_WS_URL").expect("BINANCE_WS_URL failed"),
            each.replace("-", "").to_lowercase()
        );

        subscribe_list!(
            state,
            server,
            vec![each.to_string()],
            [
                // (BinanceExchangeWSSession {}, binance_pair_url),
                // (
                //     WhitebitExchangeWSSession {},
                //     std::env::var("WHITEBIT_WS_URL").expect("WHITEBIT_WS_URL failed")
                // ),
                // (
                //     OkxExchangeWSSession {},
                //     std::env::var("OKX_WS_URL").expect("OKX_WS_URL failed")
                // ),
                (
                    BybitExchangeWSSession {},
                    std::env::var("BYBIT_WS_URL").expect("BYBIT_WS_URL failed")
                ),
                // (
                //     GateExchangeWSSession {},
                //     std::env::var("GATE_WS_URL").expect("GATE_WS_URL failed")
                // ),
                (
                    BitgetExchangeWSSession {},
                    std::env::var("BITGET_WS_URL").expect("BITGET_WS_URL failed")
                ),
                // (
                //     CryptoExchangeWSSession {},
                //     std::env::var("CRYPTO_WS_URL").expect("CRYPTO_WS_URL failed")
                // ),
            ]
        )
    }
}
