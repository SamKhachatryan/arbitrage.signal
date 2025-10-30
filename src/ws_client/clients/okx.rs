use std::{
    env,
    sync::{Arc, Mutex},
};

use futures::SinkExt;
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
            Ok(Message::Text(text)) => {
                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        continue;
                    }
                };
                if let Some(price) = parsed
                    .get("data")
                    .and_then(|data| data.get(0))
                    .and_then(|obj| obj.get("last"))
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<f64>().ok())
                {
                    let mut safe_store = state.lock().expect("Failed to lock");
                    safe_store.update_price(&pair_name, "okx", price);
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
                eprintln!("WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct OkxWSClient {}

impl ExchangeWSClient for OkxWSClient {
    async fn subscribe(state: Arc<Mutex<AppState>>, pair_name: String) {
        let url = env::var("OKX_WS_URL").expect("OKX_WS_URL not set in .env");
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
            "op": "subscribe",
            "args": [{{ "channel": "{}", "instId": "{}" }}]
        }}"#,
            "tickers".to_string(),
            pair_name.to_uppercase()
        );

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .unwrap();

        tokio::spawn(handle_ws_read(state, read, pair_name));
    }
}
