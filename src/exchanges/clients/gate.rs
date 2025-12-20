use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
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
    common, define_prometheus_counter, exchanges::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession}, state::{AppControl, AppState, PAIR_NAMES}, ws_server::WSServer
};

define_prometheus_counter!(
    GATE_UPDATES_RECEIVED_COUNTER,
    "gate_updates_received_counter",
    "Gate: Updates Received Counter"
);

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
                                    server.notify_price_change(
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
        let mut ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Gate"));
        let mut read_handle = tokio::spawn(handle_ws_read(
            state.clone(),
            server.clone(),
            read,
            write_arc.clone(),
            pair_names[0].clone(),
        ));

        let mut resync_handle = tokio::spawn(resync_orderbook_loop(
            state.clone(),
            server.clone(),
            pair_names[0].clone(),
            cancel_token.clone(),
        ));

        // Wait for either task to complete or cancellation
        tokio::select! {
            _ = &mut ping_handle => {
                eprintln!("GATE: Ping loop ended");
            }
            _ = &mut read_handle => {
                eprintln!("GATE: Read loop ended");
            }
            _ = cancel_token.cancelled() => {
                eprintln!("GATE: Cancelled - ABORTING ALL TASKS");
                ping_handle.abort();
                read_handle.abort();
                resync_handle.abort();
                drop(write_arc);
                eprintln!("GATE: All tasks aborted, connection dropped");
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

async fn handle_resync_orderbook_loop_spot(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
    pair_name: String,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let client = reqwest::Client::builder().cookie_store(true).build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[GATE] Error creating reqwest client: {}", e);
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
            eprintln!("[GATE] Resync cancelled during initial throttle for {}", pair_name);
            return;
        }
    }

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            _ = cancel_token.cancelled() => {
                eprintln!("[GATE SPOT] Resync cancelled for {}", pair_name);
                return;
            }
        }
        
        let url = std::env::var("GATE_REST_URL")
            .expect("GATE_REST_URL must be set in .env for spot pairs");

        let req = client
            .get(format!(
                "{}/spot/order_book?currency_pair={}",
                url,
                pair_name.replace("-", "_").to_uppercase(),
            ))
            .send();

        let resp = match req.await {
            Err(e) => {
                eprintln!("[GATE] Error fetching order book for {}: {}", pair_name, e);
                continue;
            }
            Ok(r) => r,
        };

        let text = match resp.text().await {
            Err(e) => {
                eprintln!(
                    "[GATE] Error reading response text for {}: {}",
                    pair_name, e
                );
                continue;
            }
            Ok(t) => t,
        };

        let parsed = match serde_json::from_str::<MarketOrderDepthREST>(&text) {
            Err(e) => {
                println!("{}", &text);
                eprintln!("[GATE] Error parsing JSON for {}: {}", pair_name, e);
                continue;
            }
            Ok(v) => v,
        };

        let state = state.lock().unwrap();

        if let Some(asks) = parsed.asks {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "gate", utc.timestamp_millis(), |order_book| {
                order_book.clean_asks();

                for ask in asks {
                    order_book.update_ask(
                        ask[0].parse::<f64>().unwrap(),
                        ask[1].parse::<f64>().unwrap(),
                    );
                }
            });
        }

        if let Some(bids) = parsed.bids {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "gate", utc.timestamp_millis(), |order_book| {
                order_book.clean_bids();

                for bid in bids {
                    order_book.update_bid(
                        bid[0].parse::<f64>().unwrap(),
                        bid[1].parse::<f64>().unwrap(),
                    );
                }
            });
        }

        server.notify_price_change(&state.exchange_price_map, &pair_name, "gate");
    }
}

#[derive(Deserialize)]
struct PerpMarketOrderDepthItemRest {
    s: f64,
    p: String,
}

#[derive(Deserialize)]
struct PerpMarketOrderDepthREST {
    asks: Option<Vec<PerpMarketOrderDepthItemRest>>,
    bids: Option<Vec<PerpMarketOrderDepthItemRest>>,
}

async fn handle_resync_orderbook_loop_perp(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
    pair_name: String,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let client = reqwest::Client::builder().cookie_store(true).build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[GATE] Error creating reqwest client: {}", e);
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
            eprintln!("[GATE] Resync cancelled during initial throttle for {}", pair_name);
            return;
        }
    }

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            _ = cancel_token.cancelled() => {
                eprintln!("[GATE] Resync cancelled for {}", pair_name);
                return;
            }
        }
        
        let url = std::env::var("GATE_REST_URL")
            .expect("GATE_REST_URL must be set in .env for spot pairs");

        let req = client
            .get(format!(
                "{}/futures/usdt/order_book?contract={}",
                url,
                pair_name.replace("-perp", "").replace("-", "_").to_uppercase(),
            ))
            .send();

        let resp = match req.await {
            Err(e) => {
                eprintln!("[GATE] Error fetching order book for {}: {}", pair_name, e);
                continue;
            }
            Ok(r) => r,
        };

        let text = match resp.text().await {
            Err(e) => {
                eprintln!(
                    "[GATE] Error reading response text for {}: {}",
                    pair_name, e
                );
                continue;
            }
            Ok(t) => t,
        };

        let parsed = match serde_json::from_str::<PerpMarketOrderDepthREST>(&text) {
            Err(e) => {
                println!("{}", &text);
                eprintln!("[GATE] Error parsing JSON for {}: {}", pair_name, e);
                continue;
            }
            Ok(v) => v,
        };

        let state = state.lock().unwrap();

        if let Some(asks) = parsed.asks {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "gate", utc.timestamp_millis(), |order_book| {
                order_book.clean_asks();

                for ask in asks {
                    order_book.update_ask(
                        ask.p.parse::<f64>().unwrap(),
                        ask.s,
                    );
                }
            });
        }

        if let Some(bids) = parsed.bids {
            let utc = Utc::now();
            state.update_order_book(&pair_name, "gate", utc.timestamp_millis(), |order_book| {
                order_book.clean_bids();

                for bid in bids {
                    order_book.update_bid(
                        bid.p.parse::<f64>().unwrap(),
                        bid.s,
                    );
                }
            });
        }

        server.notify_price_change(&state.exchange_price_map, &pair_name, "gate");
    }
}

async fn resync_orderbook_loop(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
    pair_name: String,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    if pair_name.ends_with("-perp") {
        handle_resync_orderbook_loop_perp(state, server, pair_name, cancel_token).await;
    } else {
        handle_resync_orderbook_loop_spot(state, server, pair_name, cancel_token).await;
    }
}
