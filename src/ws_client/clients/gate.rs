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
    common, define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
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

                // Order book updates: result.asks[0][0] and result.bids[0][0]
                if let Some(result) = parsed.get("result") {
                    WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                    GATE_UPDATES_RECEIVED_COUNTER.inc();

                    if let Some(i64_ts) = parsed.get("time_ms").and_then(|v| v.as_i64()) {
                        let safe_state = state.lock().expect("Failed to lock");

                        // Update order book directly without cloning
                        safe_state.update_order_book(&pair_name, "gate", i64_ts, |order_book| {
                            if let Some(ask) = result.get("a").and_then(|arr| arr.as_array()) {
                                order_book.update_ask(
                                    ask[0][0].as_str().unwrap().parse::<f64>().unwrap(),
                                    ask[0][1].as_str().unwrap().parse::<f64>().unwrap(),
                                );
                            }
                            if let Some(bid) = result.get("b").and_then(|arr| arr.as_array()) {
                                order_book.update_bid(
                                    bid[0][0].as_str().unwrap().parse::<f64>().unwrap(),
                                    bid[0][1].as_str().unwrap().parse::<f64>().unwrap(),
                                );
                            }
                        });

                        // Check depth and notify
                        if let Some(exchange_map) = safe_state.exchange_price_map.get(&pair_name) {
                            if let Some(pe) = exchange_map.get("gate") {
                                if pe.order_book.get_depth() >= 5 {
                                    if let Some(ref server_instance) = *server {
                                        server_instance.notify_price_change(
                                            &safe_state.exchange_price_map,
                                            &pair_name,
                                            "gate",
                                        );
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
        pair_names: Vec<String>,
    ) {
        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
            "time": {},
            "channel": "{}",
            "event": "subscribe",
            "payload": ["{}"]
        }}"#,
            chrono::Utc::now().timestamp(),
            if pair_names[0].ends_with("-perp") {
                "futures.obu"
            } else {
                "spot.obu"
            },
            "ob.".to_string()
                + &pair_names[0]
                    .to_uppercase()
                    .replace("-", "_")
                    .replace("_PERP", "")
                + ".50"
        );

        if let Err(e) = write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
        {
            eprintln!("GATE: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Gate"));
        let read_handle = tokio::spawn(handle_ws_read(
            state,
            server,
            read,
            write_arc.clone(),
            pair_names[0].clone(),
        ));

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
