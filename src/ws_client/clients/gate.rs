use std::sync::Arc;

use async_trait::async_trait;
use futures::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, tungstenite::{self, Message}
};

use crate::{
    define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::{clients::interface::ExchangeWSSession, common::{self}},
    ws_server::WSServer,
};

define_prometheus_counter!(
    GATE_UPDATES_RECEIVED_COUNTER,
    "gate_updates_received_counter",
    "Gate: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
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
                        let safe_state = state.lock().expect("Failed to lock");
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
            Ok(Message::Ping(message)) => {
                if let Err(_e) = write.lock().await.send(Message::Pong(message)).await {
                    eprintln!("Failed to send pong");
                }
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

pub struct GateExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for GateExchangeWSSession {
    async fn handle_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
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

        if let Err(e) = write.send(Message::Text(subscribe_msg.to_string().into())).await {
            eprintln!("GATE: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::send_ping_loop(write_arc.clone(), "Gate"));
        let read_handle = tokio::spawn(handle_ws_read(state, server, read, write_arc.clone(), pair_name));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("GATE: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("GATE: Read loop ended");
            }
        }
    }
}
