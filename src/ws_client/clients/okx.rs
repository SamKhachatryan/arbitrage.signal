use std::sync::Arc;

use async_trait::async_trait;
use futures::SinkExt;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

use crate::{
    common::{self, order_book::OrderBook},
    define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
    ws_server::WSServer,
};

define_prometheus_counter!(
    OKX_UPDATES_RECEIVED_COUNTER,
    "okx_updates_received_counter",
    "Okx: Updates Received Counter"
);

#[derive(Deserialize)]
struct MarketOrderDepth {
    ts: String,

    asks: Vec<[String; 4]>,
    bids: Vec<[String; 4]>,
}

#[derive(Deserialize)]
struct OkxResponse<T> {
    data: Option<[T; 1]>,
}

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
                let parsed = match serde_json::from_str::<OkxResponse<MarketOrderDepth>>(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} ({}) - skipping message", text, e);
                        continue;
                    }
                };

                WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                OKX_UPDATES_RECEIVED_COUNTER.inc();

                if let Some(data) = parsed.data {
                    if let Some(ts_i64) = data[0].ts.parse::<i64>().ok() {
                        let safe_state = state.lock().expect("Failed to lock");

                        // Update order book directly without cloning
                        safe_state.update_order_book(&pair_name, "okx", ts_i64, |order_book| {
                            for ask in &data[0].asks {
                                order_book.update_ask(
                                    ask[0].as_str().parse::<f64>().unwrap(),
                                    ask[1].as_str().parse::<f64>().unwrap(),
                                );
                            }

                            for bid in &data[0].bids {
                                order_book.update_bid(
                                    bid[0].as_str().parse::<f64>().unwrap(),
                                    bid[1].as_str().parse::<f64>().unwrap(),
                                );
                            }
                        });

                        // Check depth and notify
                        if let Some(exchange_map) = safe_state.exchange_price_map.get(&pair_name) {
                            if let Some(pe) = exchange_map.get("okx") {
                                if pe.order_book.get_depth() >= 5 {
                                    if let Some(ref server_instance) = *server {
                                        server_instance.notify_price_change(
                                            &safe_state.exchange_price_map,
                                            &pair_name,
                                            "okx",
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
            "books5".to_string(),
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
        let ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Okx"));
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
