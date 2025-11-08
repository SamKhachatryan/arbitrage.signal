use std::{env, sync::Arc};

use futures::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};

use crate::{
    define_prometheus_counter,
    health::prometheus::registry::METRIC_REGISTRY,
    state::{AppControl, AppState},
    ws_client::common::{self, ExchangeWSClient},
    ws_server::WSServer,
};

define_prometheus_counter!(
    CRYPTO_UPDATES_RECEIVED_COUNTER,
    "crypto_updates_received_counter",
    "Crypto: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    mut write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
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

                if let Some(method) = parsed.get("method").and_then(|name| name.as_str()) {
                    if method == "public/heartbeat" {
                        if let Some(id) = parsed.get("id").and_then(|id| id.as_i64()) {
                            let subscribe_msg = format!(
                                r#"{{
                                    "method": "public/respond-heartbeat",
                                    "id": {}
                                }}"#,
                                id
                            );
                            let _ = write.lock().await.send(Message::Text(subscribe_msg.to_string().into()));
                            continue;
                        }
                    }
                }

                let data = parsed
                    .get("result")
                    .and_then(|res| res.get("data"))
                    .and_then(|data| data.get(0));

                if let Some(val) = data {
                    if let Some(price_str) = val.get("k") {
                        if let Some(price_str) = price_str.as_str() {
                            if let Ok(price) = price_str.parse::<f64>() {
                                if let Some(i64_ts) = val.get("t").and_then(|t| t.as_i64()) {
                                    CRYPTO_UPDATES_RECEIVED_COUNTER.inc();
                                    let safe_state = state.lock().await;
                                    safe_state.update_price(&pair_name, "crypto", price, i64_ts);

                                    if let Some(ref server_instance) = *server {
                                        server_instance
                                            .notify_price_change(&safe_state.exchange_price_map);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(frame)) => {
                eprintln!("CRYPTO WebSocket closed: {:?}", frame);
                panic!();
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                // ignore pings/pongs for now
            }
            Ok(_) => {
                // other message types ignored
            }
            Err(e) => {
                eprintln!("CRYPTO WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct CryptoWSClient {}

impl ExchangeWSClient for CryptoWSClient {
    async fn subscribe(
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
        let url = env::var("CRYPTO_WS_URL").expect("CRYPTO_WS_URL not set in .env");
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
                "method": "subscribe",
                "params": {{
                    "channels": ["ticker.{}"]
                }},
                "id": 1
            }}"#,
            pair_name.to_uppercase().replace("-", "_")
        );

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .unwrap();

        let write_arc = Arc::new(Mutex::new(write));

        tokio::spawn(common::send_ping_loop_async(write_arc.clone(), "Crypto"));
        tokio::spawn(handle_ws_read(state, server, read, write_arc.clone(), pair_name));
    }
}
