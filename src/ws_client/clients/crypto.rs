use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::Mutex, time::sleep};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

use crate::{
    define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::{
        clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
        common,
    },
    ws_server::WSServer,
};

define_prometheus_counter!(
    CRYPTO_UPDATES_RECEIVED_COUNTER,
    "crypto_updates_received_counter",
    "Crypto: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
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
                            let _ = write
                                .lock()
                                .await
                                .send(Message::Text(subscribe_msg.to_string().into()));
                            continue;
                        }
                    }
                }

                let result = parsed.get("result");

                let data = result
                    .and_then(|res| res.get("data"))
                    .and_then(|data| data.get(0));

                if let Some(instrument) = result
                    .and_then(|data| data.get("instrument_name"))
                    .and_then(|v| v.as_str())
                {
                    let pair_name = instrument.replace("_", "-").to_lowercase();
    
                    if let Some(val) = data {
                        if let Some(price_str) = val.get("k") {
                            if let Some(price_str) = price_str.as_str() {
                                if let Ok(price) = price_str.parse::<f64>() {
                                    if let Some(i64_ts) = val.get("t").and_then(|t| t.as_i64()) {
                                        WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                                        CRYPTO_UPDATES_RECEIVED_COUNTER.inc();
                                        let safe_state = state.lock().expect("Failed to lock");
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
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(Some(frame))) => {
                eprintln!(
                    "CRYPTO Closed: code = {:?}, reason = {:?}",
                    frame.code, frame.reason
                );
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
                eprintln!("CRYPTO WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct CryptoExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for CryptoExchangeWSSession {
    async fn handle_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_names: Vec<String>,
    ) {
        let (write, read) = ws_stream.split();
        let write_arc = Arc::new(Mutex::new(write));

        sleep(Duration::from_secs(5)).await;

        for each in pair_names {
            let subscribe_msg = format!(
                r#"{{ "method": "subscribe", "params": {{ "channels": ["ticker.{}"] }}, "id": 1 }}"#,
                each.to_uppercase().replace("-", "_")
            );

            if let Err(e) = write_arc
                .clone()
                .lock()
                .await
                .send(Message::Text(subscribe_msg.into()))
                .await
            {
                eprintln!("CRYPTO: Failed to send subscribe: {}", e);
                return;
            }
        }

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::send_ping_loop(write_arc.clone(), "Crypto"));
        let read_handle = tokio::spawn(handle_ws_read(
            state.clone(),
            server.clone(),
            read,
            write_arc.clone(),
        ));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("CRYPTO: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("CRYPTO: Read loop ended");
            }
        }
    }
}
