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
    common, define_prometheus_counter,
    exchanges::clients::{
        WS_CLIENTS_PACKAGES_RECEIVED_COUNTER,
        interface::{ExchangeResyncOrderbook, ExchangeWSSession},
    },
    state::{AppControl, AppState, PAIR_NAMES},
    ws_server::WSServer,
};

define_prometheus_counter!(
    BITGET_UPDATES_RECEIVED_COUNTER,
    "bitget_updates_received_counter",
    "Bitget: Updates Received Counter"
);

#[derive(Deserialize)]
struct MarketOrderDepth {
    ts: String,

    asks: Vec<[String; 2]>,
    bids: Vec<[String; 2]>,
}

#[derive(Deserialize)]
struct BitgetResponse<T> {
    data: Option<[T; 1]>,
}

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<WSServer>,
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

                WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                BITGET_UPDATES_RECEIVED_COUNTER.inc();

                let parsed = match serde_json::from_str::<BitgetResponse<MarketOrderDepth>>(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        continue;
                    }
                };

                if let Some(data) = parsed.data {
                    if let Some(ts_i64) = data[0].ts.parse::<i64>().ok() {
                        let safe_state = state.lock().expect("Failed to lock");

                        // Update order book directly without cloning
                        safe_state.update_order_book(&pair_name, "bitget", ts_i64, |order_book| {
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
                            if let Some(pe) = exchange_map.get("bitget") {
                                if pe.order_book.get_depth() >= 5 {
                                    server.notify_price_change(
                                            &safe_state.exchange_price_map,
                                            &pair_name,
                                            "bitget",
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
    async fn handle_ws_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<WSServer>,
        pair_names: Vec<String>,
    ) {
        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
                "op": "subscribe",
                "args": [
                    {{
                        "instType": "{}",
                        "channel": "books5",
                        "instId": "{}"
                    }}
                ]
            }}"#,
            if pair_names[0].ends_with("-perp") {
                "USDT-FUTURES"
            } else {
                "SPOT"
            },
            pair_names[0]
                .to_uppercase()
                .replace("-PERP", "")
                .replace("-", "")
        );

        if let Err(e) = write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
        {
            eprintln!("BITGET: Failed to send subscription message: {}", e);
            return;
        }

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Bitget"));
        let read_handle = tokio::spawn(handle_ws_read(
            state.clone(),
            server.clone(),
            read,
            write_arc.clone(),
            pair_names[0].to_string(),
        ));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("[BITGET] Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("[BITGET] Read loop ended");
            }
            _ = self.resync_orderbook_loop(state.clone(), server.clone(), pair_names[0].to_string()) => {
                eprintln!("[BITGET] Read loop ended");
            }
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum FloatOrString {
    Float(f64),
    String(String),
}

#[derive(Deserialize)]
struct MarketOrderDepthREST {
    asks: Option<Vec<[FloatOrString; 2]>>,
    bids: Option<Vec<[FloatOrString; 2]>>,
}

#[derive(Deserialize)]
struct BitgetResponseREST<T> {
    data: T,
}

#[async_trait]
impl ExchangeResyncOrderbook for BitgetExchangeWSSession {
    async fn resync_orderbook_loop(
        &self,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<WSServer>,
        pair_name: String,
    ) {
        let client = reqwest::Client::builder().cookie_store(true).build();

        let client = match client {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[BITGET] Error creating reqwest client: {}", e);
                return;
            }
        };

        let pair_index = PAIR_NAMES.iter().position(|p| p == &pair_name);
        let per_pair_ms = 5000 / PAIR_NAMES.len() as u64;

        // TODO: Actual good queue/semaphore throttling needed here
        let throttle_duration: std::time::Duration =
            std::time::Duration::from_millis(per_pair_ms * (pair_index.unwrap_or(0) + 1) as u64);

        tokio::time::sleep(throttle_duration).await;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let url = std::env::var("BITGET_REST_URL")
                .expect("BITGET_REST_URL must be set in .env for spot pairs");

            let endpoint = if pair_name.ends_with("-perp") {
                "mix/market/merge-depth?limit=5&productType=usdt-futures&precision=scale0"
            } else {
                "spot/market/orderbook?limit=5"
            };

            let req = client.get(format!(
                "{}/{}&symbol={}",
                url,
                endpoint,
                pair_name
                    .replace("-perp", "")
                    .replace("-", "")
                    .to_uppercase(),
            )).send();

            let resp = match req.await {
                Err(e) => {
                    eprintln!(
                        "[BITGET] Error fetching order book for {}: {}",
                        pair_name, e
                    );
                    continue;
                }
                Ok(r) => r,
            };

            let text = match resp.text().await {
                Err(e) => {
                    eprintln!(
                        "[BITGET] Error reading response text for {}: {}",
                        pair_name, e
                    );
                    continue;
                }
                Ok(t) => t,
            };

            let parsed =
                match serde_json::from_str::<BitgetResponseREST<MarketOrderDepthREST>>(&text) {
                    Err(e) => {
                        println!("{}", &text);
                        eprintln!("[BITGET] Error parsing JSON for {}: {}", pair_name, e);
                        continue;
                    }
                    Ok(v) => v,
                };

            let state = state.lock().unwrap();

            if let Some(asks) = parsed.data.asks {
                let utc = Utc::now();
                state.update_order_book(
                    &pair_name,
                    "bitget",
                    utc.timestamp_millis(),
                    |order_book| {
                        order_book.clean_asks();

                        for ask in asks {
                            order_book.update_ask(
                                match ask[0] {
                                    FloatOrString::Float(f) => f,
                                    FloatOrString::String(ref s) => s.parse::<f64>().unwrap_or(0.0),
                                },
                                match ask[1] {
                                    FloatOrString::Float(f) => f,
                                    FloatOrString::String(ref s) => s.parse::<f64>().unwrap_or(0.0),
                                },
                            );
                        }
                    },
                );
            }

            if let Some(bids) = parsed.data.bids {
                let utc = Utc::now();
                state.update_order_book(
                    &pair_name,
                    "bitget",
                    utc.timestamp_millis(),
                    |order_book| {
                        order_book.clean_bids();

                        for bid in bids {
                            order_book.update_bid(
                                match bid[0] {
                                    FloatOrString::Float(f) => f,
                                    FloatOrString::String(ref s) => s.parse::<f64>().unwrap_or(0.0),
                                },
                                match bid[1] {
                                    FloatOrString::Float(f) => f,
                                    FloatOrString::String(ref s) => s.parse::<f64>().unwrap_or(0.0),
                                },
                            );
                        }
                    },
                );
            }

            server.notify_price_change(
                    &state.exchange_price_map,
                    &pair_name,
                    "bitget",
                );
        }
    }
}
