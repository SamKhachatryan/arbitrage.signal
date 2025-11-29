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
    OKX_UPDATES_RECEIVED_COUNTER,
    "okx_updates_received_counter",
    "Okx: Updates Received Counter"
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

                if let Some(data) = parsed.get("data").and_then(|data| data.get(0)) {
                    match data
                        .get("last")
                        .and_then(|v| v.as_str())
                        .and_then(|v| v.parse::<f64>().ok())
                    {
                        Some(price) => {
                            if let Some(ts) = data.get("ts") {
                                if let Some(ts_str) = ts.as_str() {
                                    if let Ok(i64_ts) = ts_str.parse::<i64>() {
                                        WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                                        OKX_UPDATES_RECEIVED_COUNTER.inc();
                                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                                        println!("OKX - {}", now - i64_ts);
                                        let safe_state = state.lock().expect("Failed to lock");
                                        safe_state.update_price(&pair_name, "okx", price, i64_ts);

                                        if let Some(ref server_instance) = *server {
                                            server_instance.notify_price_change(
                                                &safe_state.exchange_price_map,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        None => eprintln!("Failed to parse price okx"),
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(frame)) => {
                eprintln!("OKX WebSocket closed: {:?}", frame);
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
                eprintln!("OKX WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct OkxExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for OkxExchangeWSSession {
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
            "op": "subscribe",
            "args": [{{ "channel": "{}", "instId": "{}" }}]
        }}"#,
            "tickers".to_string(),
            pair_names[0].replace("-perp", "-swap").to_uppercase()
        );

        if let Err(e) = write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
        {
            eprintln!("OKX: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::send_ping_loop(write_arc.clone(), "Okx"));
        let read_handle = tokio::spawn(handle_ws_read(
            state,
            server,
            read,
            write_arc.clone(),
            pair_names[0].to_string(),
        ));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("OKX: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("OKX: Read loop ended");
            }
        }
    }
}
