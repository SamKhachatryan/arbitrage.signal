use std::{
    env,
    sync::{Arc},
};

use futures::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};

use crate::{
    define_prometheus_counter, health::prometheus::registry::METRIC_REGISTRY, state::{AppControl, AppState}, ws_client::common::{self, ExchangeWSClient}, ws_server::WSServer
};

define_prometheus_counter!(WHITEBIT_UPDATES_RECEIVED_COUNTER, "whitebit_updates_received_counter", "WHITEBIT: Updates Received Counter");

async fn handle_ws_read(
    state: Arc<Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    //ui: Arc<Mutex<AppState>>,
    pair_name: String,
) {
    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        continue;
                    }
                };

                let working_params = parsed.get("params").and_then(|params| params[0].as_array());

                if let Some(params) = working_params {
                    let bid_price = params[4].as_str().and_then(|s| s.parse::<f64>().ok());

                    let ask_price = params[6].as_str().and_then(|s| s.parse::<f64>().ok());

                    if let (Some(bid_price), Some(ask_price)) = (bid_price, ask_price) {
                        let mid = (bid_price + ask_price) / 2.0;

                        let ts_f64 = params[0].as_f64();

                        if let Some(ts_f64) = ts_f64 {
                            WHITEBIT_UPDATES_RECEIVED_COUNTER.inc();
                            
                            let i64_ts = (ts_f64 * 1000.0) as i64;

                            let safe_state = state.lock().await;
                            safe_state.update_price(&pair_name, "whitebit", mid, i64_ts);

                            if let Some(ref server_instance) = *server {
                                server_instance.notify_price_change(&safe_state.exchange_price_map);
                            }
                        }
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(frame)) => {
                eprintln!("WebSocket closed: {:?}", frame);
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                // ignore pings/pongs for now
            }
            Ok(_) => {
                // other message types ignored
            }
            Err(e) => {
                eprintln!("WHITEBIT WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct WhitebitWSClient {}

impl ExchangeWSClient for WhitebitWSClient {
    async fn subscribe(
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
        let url = env::var("WHITEBIT_WS_URL").expect("WHITEBIT_WS_URL not set in .env");
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
                "id": 1,
                "method": "bookTicker_subscribe",
                "params": ["{}"]
            }}"#,
            pair_name.to_uppercase().replace("-", "_")
        );

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .unwrap();

        tokio::spawn(common::send_ping_loop(write, "WhitebitW"));
        tokio::spawn(handle_ws_read(state, server, read, pair_name));
    }
}
