use std::sync::Arc;

use async_trait::async_trait;
use futures::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

use crate::{
    define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::{clients::interface::ExchangeWSSession, common},
    ws_server::WSServer,
};

define_prometheus_counter!(
    BITGET_UPDATES_RECEIVED_COUNTER,
    "bitget_updates_received_counter",
    "Bitget: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
    //ui: Arc<Mutex<AppState>>,
    pair_name: String,
) {
    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                if text.contains("\"op\":\"ping\"") {
                    let pong = "{\"op\":\"pong\"}";
                    let _ = write.lock().await.send(Message::Text(pong.into())).await;
                    continue;
                }

                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        continue;
                    }
                };

                let data = parsed.get("data").and_then(|data| data.get(0));

                if let Some(val) = data {
                    if let Some(price_str) = val.get("lastPr") {
                        if let Some(price_str) = price_str.as_str() {
                            if let Ok(price) = price_str.parse::<f64>() {
                                if let Some(ts) = val.get("ts") {
                                    if let Some(ts_str) = ts.as_str() {
                                        if let Ok(i64_ts) = ts_str.parse::<i64>() {
                                            BITGET_UPDATES_RECEIVED_COUNTER.inc();
                                            let safe_state = state.lock().expect("Failed to lock");
                                            safe_state
                                                .update_price(&pair_name, "bitget", price, i64_ts);

                                            if let Some(ref server_instance) = *server {
                                                server_instance.notify_price_change(
                                                    &safe_state.exchange_price_map,
                                                );
                                            }
                                        }
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
                eprintln!("BITGET WebSocket closed: {:?}", frame);
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
                eprintln!("BITGET WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct BitgetExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for BitgetExchangeWSSession {
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
                "op": "subscribe",
                "args": [
                    {{
                        "instType": "SPOT",
                        "channel": "ticker",
                        "instId": "{}"
                    }}
                ]
            }}"#,
            pair_name.to_uppercase().replace("-", "")
        );

        if let Err(e) = write.send(Message::Text(subscribe_msg.to_string().into())).await {
            eprintln!("BITGET: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::send_ping_loop(write_arc.clone(), "Bitget"));
        let read_handle = tokio::spawn(handle_ws_read(state, server, read, write_arc.clone(), pair_name));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("BITGET: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("BITGET: Read loop ended");
            }
        }
    }
}
