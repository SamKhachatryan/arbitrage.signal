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

define_prometheus_counter!(GATE_UPDATES_RECEIVED_COUNTER, "gate_updates_received_counter", "Gate: Updates Received Counter");

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

                if let Some(price) = parsed
                    .get("result")
                    .and_then(|data| data.get("last"))
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<f64>().ok())
                {
                    if let Some(i64_ts) = parsed.get("time_ms").and_then(|v| v.as_i64()) {
                        GATE_UPDATES_RECEIVED_COUNTER.inc();
                        let safe_state = state.lock().await;
                        safe_state.update_price(&pair_name, "gate", price, i64_ts);
                        if let Some(ref server_instance) = *server {
                            server_instance.notify_price_change(&safe_state.exchange_price_map);
                        }
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(frame)) => {
                eprintln!("GATE WebSocket closed: {:?}", frame);
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                // ignore pings/pongs for now
            }
            Ok(_) => {
                // other message types ignored
            }
            Err(e) => {
                eprintln!("GATE WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct GateWSClient {}

impl ExchangeWSClient for GateWSClient {
    async fn subscribe(
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
        let url = env::var("GATE_WS_URL").expect("GATE_WS_URL not set in .env");
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
            "time": {},
            "channel": "spot.tickers",
            "event": "subscribe",
            "payload": ["{}"]
        }}"#,
            chrono::Utc::now().timestamp(),
            pair_name.to_uppercase().replace("-", "_")
        );

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .unwrap();

        tokio::spawn(common::send_ping_loop(write, "Gate"));
        tokio::spawn(handle_ws_read(state, server, read, pair_name));
    }
}
