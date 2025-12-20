use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
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
    exchanges::clients::{
        WS_CLIENTS_PACKAGES_RECEIVED_COUNTER,
        interface::ExchangeWSSession,
    },
    state::{AppControl, AppState},
    ws_server::WSServer,
};

define_prometheus_counter!(
    WHITEBIT_UPDATES_RECEIVED_COUNTER,
    "whitebit_updates_received_counter",
    "WHITEBIT: Updates Received Counter"
);

#[derive(Deserialize)]
struct MarketOrderDepth {
    timestamp: f64,

    asks: Option<Vec<[String; 2]>>,
    bids: Option<Vec<[String; 2]>>,
}

#[derive(Deserialize)]
struct WhitebitResponse<T> {
    params: Option<(bool, T, String)>,
}

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
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
                    let ts_i64 = data.1.timestamp as i64;
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
                                server.notify_price_change(
                                    &safe_state.exchange_price_map,
                                    &pair_name,
                                    "whitebit",
                                );
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
    async fn handle_ws_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<WSServer>,
        pair_names: Vec<String>,
        cancel_token: tokio_util::sync::CancellationToken,
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
        let mut ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "WHITEBIT"));
        let mut read_handle = tokio::spawn(handle_ws_read(
            state.clone(),
            server.clone(),
            read,
            write_arc.clone(),
            pair_names[0].clone(),
        ));

        let resync_handle = tokio::spawn(resync_orderbook_loop(
            state.clone(),
            server.clone(),
            pair_names[0].clone(),
            cancel_token.clone(),
        ));

        // Wait for either task to complete or cancellation
        tokio::select! {
            _ = &mut ping_handle => {
                eprintln!("WHITEBIT: Ping loop ended");
            }
            _ = &mut read_handle => {
                eprintln!("WHITEBIT: Read loop ended");
            }
            _ = cancel_token.cancelled() => {
                eprintln!("WHITEBIT: Cancelled - ABORTING ALL TASKS");
                ping_handle.abort();
                read_handle.abort();
                resync_handle.abort();
                drop(write_arc);
                eprintln!("WHITEBIT: All tasks aborted, connection dropped");
                return;
            }
        }
    }
}

#[derive(Deserialize)]
struct MarketOrderDepthREST {
    asks: Option<Vec<[String; 2]>>,
    bids: Option<Vec<[String; 2]>>,
}

async fn resync_orderbook_loop(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
    pair_name: String,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {},
            _ = cancel_token.cancelled() => {
                eprintln!("[WHITEBIT] Resync cancelled for {}", pair_name);
                return;
            }
        }

        let url = std::env::var("WHITEBIT_REST_URL")
                .expect("WHITEBIT_REST_URL must be set in .env for spot pairs");

        let req = reqwest::get(format!(
            "{}/public/orderbook/{}?&limit=5",
            url,
            pair_name
                .replace("usdt-perp", "perp")
                .replace("-", "_")
                .to_uppercase(),
        ));

        let resp = match req.await {
            Err(e) => {
                eprintln!(
                    "[WHITEBIT] Error fetching order book for {}: {}",
                    pair_name, e
                );
                continue;
            }
            Ok(r) => r,
        };

        let text = match resp.text().await {
            Err(e) => {
                eprintln!(
                    "[WHITEBIT] Error reading response text for {}: {}",
                    pair_name, e
                );
                continue;
            }
            Ok(t) => t,
        };

        let parsed = match serde_json::from_str::<MarketOrderDepthREST>(&text) {
            Err(e) => {
                eprintln!("[WHITEBIT] Error parsing JSON for {}: {}", pair_name, e);
                continue;
            }
            Ok(v) => v,
        };

        let state = state.lock().unwrap();

        if let Some(asks) = parsed.asks {
            let utc = Utc::now();
            state.update_order_book(
                &pair_name,
                "whitebit",
                utc.timestamp_millis(),
                |order_book| {
                    order_book.clean_asks();

                    for ask in asks {
                        order_book.update_ask(
                            ask[0].as_str().parse::<f64>().unwrap(),
                            ask[1].as_str().parse::<f64>().unwrap(),
                        );
                    }
                },
            );
        }

        if let Some(bids) = parsed.bids {
            let utc = Utc::now();
            state.update_order_book(
                &pair_name,
                "whitebit",
                utc.timestamp_millis(),
                |order_book| {
                    order_book.clean_bids();

                    for bid in bids {
                        order_book.update_bid(
                            bid[0].as_str().parse::<f64>().unwrap(),
                            bid[1].as_str().parse::<f64>().unwrap(),
                        );
                    }
                },
            );
        }

        server.notify_price_change(&state.exchange_price_map, &pair_name, "whitebit");
    }
}
