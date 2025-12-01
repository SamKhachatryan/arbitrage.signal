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

#[derive(Deserialize)]
struct MarketOrderDepth {
    #[serde(rename = "E")]
    t: i64,

    a: Vec<[String; 2]>,
    b: Vec<[String; 2]>,
}

define_prometheus_counter!(
    BINANCE_UPDATES_RECEIVED_COUNTER,
    "binance_updates_received_counter",
    "Binance: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<std::sync::Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
    pair_name: String,
) {
    let mut order_book = OrderBook::new();

    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let parsed: MarketOrderDepth = match serde_json::from_str(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        println!("{}", parsed);
                        continue;
                    }
                };

                WS_CLIENTS_PACKAGES_RECEIVED_COUNTER.inc();
                BINANCE_UPDATES_RECEIVED_COUNTER.inc();

                for ask in &parsed.a {
                    order_book.update_ask(
                        ask[0].as_str().parse::<f64>().unwrap(),
                        ask[1].as_str().parse::<f64>().unwrap(),
                    );
                }

                for bid in &parsed.b {
                    order_book.update_bid(
                        bid[0].as_str().parse::<f64>().unwrap(),
                        bid[1].as_str().parse::<f64>().unwrap(),
                    );

                    if order_book.get_depth() < 5 {
                        continue; // wait until we have enough depth
                    }

                    let safe_state = state.lock().expect("Failed to lock");
                    if let Some(mid) = order_book.get_mid_price() {
                        safe_state.update_price(&pair_name, "binance", mid, parsed.t);

                        if let Some(ref server_instance) = *server {
                            server_instance.notify_price_change(&safe_state.exchange_price_map);
                        }
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                // ignore binary messages
            }
            Ok(Message::Close(frame)) => {
                eprintln!("BINANCE WebSocket closed: {:?}", frame);
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
                eprintln!("BINANCE WebSocket read error: {}", e);
                // stop the read loop on error; higher-level code can decide to reconnect
                break;
            }
        }
    }
}

pub struct BinanceExchangeWSSession {}

#[async_trait]
impl ExchangeWSSession for BinanceExchangeWSSession {
    async fn handle_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<std::sync::Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_names: Vec<String>,
    ) {
        let (write, read) = ws_stream.split();

        let write_arc = Arc::new(Mutex::new(write));

        // Spawn both tasks and wait for either to complete
        let ping_handle = tokio::spawn(common::ping::send_ping_loop(write_arc.clone(), "Binance"));
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
                eprintln!("BINANCE: Ping loop ended");
            }
            _ = read_handle => {
                eprintln!("BINANCE: Read loop ended");
            }
        }
    }
}
