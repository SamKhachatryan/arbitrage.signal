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
    ws_client::{
        clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
        common::{self},
    },
    ws_server::WSServer,
};

define_prometheus_counter!(
    WHITEBIT_UPDATES_RECEIVED_COUNTER,
    "whitebit_updates_received_counter",
    "WHITEBIT: Updates Received Counter"
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

                let working_params = parsed.get("params").and_then(|params| params[0].as_array());

                if let Some(params) = working_params {
                    let bid_price = params[4].as_str().and_then(|s| s.parse::<f64>().ok());

                    let ask_price = params[6].as_str().and_then(|s| s.parse::<f64>().ok());

                    if let (Some(bid_price), Some(ask_price)) = (bid_price, ask_price) {
                        let mid = (bid_price + ask_price) / 2.0;

                        let ts_f64 = params[0].as_f64();

                        if let Some(ts_f64) = ts_f64 {
                            WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                            WHITEBIT_UPDATES_RECEIVED_COUNTER.inc();

                            let i64_ts = (ts_f64 * 1000.0) as i64;

                            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                            println!("Whitebit - {}", now - i64_ts);
                            let safe_state = state.lock().expect("Failed to lock");
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
                eprintln!("WHITEBIT WebSocket closed: {:?}", frame);
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
                eprintln!("WHITEBIT WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct WhitebitExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for WhitebitExchangeWSSession {
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
                "id": 1,
                "method": "bookTicker_subscribe",
                "params": ["{}"]
            }}"#,
            if pair_names[0].ends_with("-perp") {
                pair_names[0]
                    .replace("-usdt", "")
                    .to_uppercase()
                    .replace("-", "_")
            } else {
                pair_names[0].to_uppercase().replace("-", "_")
            }
        );

        if let Err(e) = write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
        {
            eprintln!("WHITEBIT: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::send_ping_loop(write_arc.clone(), "Whitebit"));
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
                eprintln!("WHITEBIT: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("WHITEBIT: Read loop ended");
            }
        }
    }
}
