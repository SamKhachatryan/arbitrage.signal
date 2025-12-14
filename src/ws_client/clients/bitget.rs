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
    common, define_prometheus_counter,
    state::{AppControl, AppState},
    ws_client::clients::{WS_CLIENTS_PACKAGES_RECEIVED_COUNTER, interface::ExchangeWSSession},
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
    server: Arc<Option<WSServer>>,
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
                                    if let Some(ref server_instance) = *server {
                                        server_instance.notify_price_change(
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
            state,
            server,
            read,
            write_arc.clone(),
            pair_names[0].to_string(),
        ));

        // Wait for either task to complete (whichever finishes first indicates connection is done)
        tokio::select! {
            _ = ping_handle => {
                eprintln!("BITGET: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("BITGET: Read loop ended");
            }
        }
    }
}
