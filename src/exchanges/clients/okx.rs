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
    exchanges::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
    state::{AppControl, AppState, PAIR_NAMES},
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
    server: Arc<WSServer>,
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
                                    server.notify_price_change(
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
            "op": "subscribe",
            "args": [{{ "channel": "books5", "instId": "{}" }}]
        }}"#,
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
        let mut ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Okx"));
        let mut read_handle = tokio::spawn(handle_ws_read(
            state.clone(),
            server.clone(),
            read,
            write_arc.clone(),
            pair_names[0].to_string(),
        ));

        let mut resync_handle = tokio::spawn(resync_orderbook_loop(
            state.clone(),
            server.clone(),
            pair_names[0].to_string(),
            cancel_token.clone(),
        ));

        // Wait for either task to complete or cancellation
        tokio::select! {
            _ = &mut ping_handle => {
                eprintln!("OKX: Ping loop ended");
            }
            _ = &mut read_handle => {
                eprintln!("OKX: Read loop ended");
            }
            _ = cancel_token.cancelled() => {
                eprintln!("OKX: Cancelled - ABORTING ALL TASKS");
                ping_handle.abort();
                read_handle.abort();
                resync_handle.abort();
                drop(write_arc);
                eprintln!("OKX: All tasks aborted, connection dropped");
                return;
            }
        }
    }
}

#[derive(Deserialize)]
struct MarketOrderDepthREST {
    asks: Option<Vec<[String; 4]>>,
    bids: Option<Vec<[String; 4]>>,
}

#[derive(Deserialize)]
struct OkxResponseREST<T> {
    data: Vec<T>,
}

async fn resync_orderbook_loop(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
    pair_name: String,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let client = reqwest::Client::builder().cookie_store(true).build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[OKX] Error creating reqwest client: {}", e);
            return;
        }
    };

    let pair_index = PAIR_NAMES.iter().position(|p| p == &pair_name);
    let per_pair_ms = 5000 / PAIR_NAMES.len() as u64;

    // TODO: Actual good queue/semaphore throttling needed here
    let throttle_duration: std::time::Duration =
        std::time::Duration::from_millis(per_pair_ms * (pair_index.unwrap_or(0) + 1) as u64);

    tokio::select! {
        _ = tokio::time::sleep(throttle_duration) => {}
        _ = cancel_token.cancelled() => {
            eprintln!("[OKX] Resync cancelled during initial throttle for {}", pair_name);
            return;
        }
    }

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            _ = cancel_token.cancelled() => {
                eprintln!("[OKX] Resync cancelled for {}", pair_name);
                return;
            }
        }
        
        let url =
            std::env::var("OKX_REST_URL").expect("OKX_REST_URL must be set in .env for spot pairs");

        let req = client
            .get(format!(
                "{}/market/books?sz=5&instId={}",
                url,
                pair_name.replace("-perp", "-swap").to_uppercase(),
            ))
            .send();

        let resp = match req.await {
            Err(e) => {
                eprintln!("[OKX] Error fetching order book for {}: {}", pair_name, e);
                continue;
            }
            Ok(r) => r,
        };

        let text = match resp.text().await {
            Err(e) => {
                eprintln!("[OKX] Error reading response text for {}: {}", pair_name, e);
                continue;
            }
            Ok(t) => t,
        };

        let parsed = match serde_json::from_str::<OkxResponseREST<MarketOrderDepthREST>>(&text) {
            Err(e) => {
                println!("{}", &text);
                eprintln!("[OKX] Error parsing JSON for {}: {}", pair_name, e);
                continue;
            }
            Ok(v) => v,
        };

        let state = state.lock().unwrap();

        if let Some(asks) = &parsed.data[0].asks {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "okx", utc.timestamp_millis(), |order_book| {
                order_book.clean_asks();

                for ask in asks {
                    order_book.update_ask(
                        ask[0].parse::<f64>().unwrap_or(0.0),
                        ask[1].parse::<f64>().unwrap_or(0.0),
                    );
                }
            });
        }

        if let Some(bids) = &parsed.data[0].bids {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "okx", utc.timestamp_millis(), |order_book| {
                order_book.clean_bids();

                for bid in bids {
                    order_book.update_bid(
                        bid[0].parse::<f64>().unwrap_or(0.0),
                        bid[1].parse::<f64>().unwrap_or(0.0),
                    );
                }
            });
        }

        server.notify_price_change(&state.exchange_price_map, &pair_name, "okx");
    }
}
