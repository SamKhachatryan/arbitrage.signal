use std::sync::Arc;

use async_trait::async_trait;
use futures::SinkExt;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

use crate::{
    common::{self},
    define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
    ws_server::WSServer,
};

define_prometheus_counter!(
    WHITEBIT_UPDATES_RECEIVED_COUNTER,
    "whitebit_updates_received_counter",
    "WHITEBIT: Updates Received Counter"
);

#[derive(Deserialize)]
struct MarketOrderDepth {
    event_time: f64,

    asks: Option<Vec<[String; 2]>>,
    bids: Option<Vec<[String; 2]>>,
}

#[derive(Deserialize)]
struct WhitebitResponse<T> {
    params: Option<(bool, T, String)>,
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
                let parsed = match serde_json::from_str::<WhitebitResponse<MarketOrderDepth>>(&text)
                {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} ({}) - skipping message", text, e);
                        continue;
                    }
                };

                WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                WHITEBIT_UPDATES_RECEIVED_COUNTER.inc();

                if let Some(data) = parsed.params {
                    let ts_i64 = data.1.event_time as i64;
                    let safe_state = state.lock().expect("Failed to lock");

                    // Update order book directly without cloning
                    safe_state.update_order_book(&pair_name, "whitebit", ts_i64, |order_book| {
                        match &data.1.asks {
                            Some(asks) => {
                                for ask in asks {
                                    order_book.update_ask(
                                        ask[0].as_str().parse::<f64>().unwrap(),
                                        ask[1].as_str().parse::<f64>().unwrap(),
                                    );
                                }
                            }
                            None => {}
                        }

                        match &data.1.bids {
                            Some(bids) => {
                                for bid in bids {
                                    order_book.update_bid(
                                        bid[0].as_str().parse::<f64>().unwrap(),
                                        bid[1].as_str().parse::<f64>().unwrap(),
                                    );
                                }
                            }
                            None => {}
                        }
                    });

                    // Check depth and notify
                    if let Some(exchange_map) = safe_state.exchange_price_map.get(&pair_name) {
                        if let Some(pe) = exchange_map.get("whitebit") {
                            if pe.order_book.get_depth() >= 5 {
                                if let Some(ref server_instance) = *server {
                                    server_instance.notify_price_change(
                                        &safe_state.exchange_price_map,
                                        &pair_name,
                                        "whitebit",
                                    );
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
                "method": "depth_subscribe",
                "params": ["{}", 5, "0", true]
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
        let ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Whitebit"));
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
