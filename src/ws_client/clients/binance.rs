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
    ws_client::common::ExchangeWSClient,
};

async fn handle_ws_read(
    state: Arc<Mutex<AppState>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    //ui: Arc<Mutex<AppState>>,
    pair_name: String,
) {
    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => match serde_json::from_str::<Value>(&text) {
                Ok(safe_value) => match safe_value.get("p").and_then(|v| v.as_str()) {
                    Some(price_str) => match price_str.parse::<f64>() {
                        Ok(decimal_f64) => {
                            let mut safe_store = state.lock().expect("Failed to lock");
                            safe_store.update_price(&pair_name, "binance", decimal_f64);
                        }
                        Err(e) => eprintln!("Failed to parse price '{}' as f64: {}", price_str, e),
                    },
                    None => eprintln!("Price field missing: skipping message"),
                },
                Err(e) => eprintln!("Error parsing JSON: {} - skipping message", e),
            },
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
    async fn subscribe(state: Arc<Mutex<AppState>>, pair_name: String) {
        let url = env::var("BINANCE_WS_URL").expect("BINANCE_WS_URL not set in .env");
        let pair_url = format!(
            "{}/{}@trade",
            url,
            pair_name.replace("-", "").to_lowercase()
        );
        let (ws_stream, _) = connect_async(pair_url).await.expect("Failed to connect");

        let (_, read) = ws_stream.split();

        tokio::spawn(handle_ws_read(state, read, pair_name));
    }
}
