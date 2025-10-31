use std::{
    env,
    sync::{Arc, Mutex},
};

use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};

use crate::{
    state::{AppControl, AppState},
    ws_client::common::{self, ExchangeWSClient},
    ws_server::WSServer,
};

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

                match parsed.get("p").and_then(|v| v.as_str()) {
                    Some(price_str) => match price_str.parse::<f64>() {
                        Ok(price) => {
                            if let Some(ts) = parsed.get("T") {
                                if let Some(i64_ts) = ts.as_i64() {
                                    let mut safe_state = state.lock().expect("Failed to lock");
                                    safe_state.update_price(&pair_name, "binance", price, i64_ts);

                                    if let Some(ref server_instance) = *server {
                                        server_instance
                                            .notify_price_change(&safe_state.exchange_price_map);
                                    }
                                }
                            }
                        },
                        Err(_) => eprintln!("Failed to parse price as f64")
                    },
                    None => eprintln!("Failed to parse price as str"),
                };
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
                eprintln!("WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct BinanceWSClient {}

impl ExchangeWSClient for BinanceWSClient {
    async fn subscribe(
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
        let url = env::var("BINANCE_WS_URL").expect("BINANCE_WS_URL not set in .env");
        let pair_url = format!(
            "{}/{}@trade",
            url,
            pair_name.replace("-", "").to_lowercase()
        );
        let (ws_stream, _) = connect_async(pair_url).await.expect("Failed to connect");

        let (write, read) = ws_stream.split();

        tokio::spawn(common::send_ping_loop(write));
        tokio::spawn(handle_ws_read(state, server, read, pair_name));
    }
}
