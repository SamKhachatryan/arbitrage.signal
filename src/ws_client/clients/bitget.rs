use std::{
    env,
    sync::{Arc},
};

use futures::{SinkExt, stream::SplitSink};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, Message},
};

use crate::{
    define_prometheus_counter,
    health::prometheus::registry::METRIC_REGISTRY,
    state::{AppControl, AppState},
    ws_client::common::{ExchangeWSClient},
    ws_server::WSServer,
};

define_prometheus_counter!(
    BITGET_UPDATES_RECEIVED_COUNTER,
    "bitget_updates_received_counter",
    "Bitget: Updates Received Counter"
);

async fn handle_ws_read(
    state: Arc<Mutex<AppState>>,
    server: Arc<Option<WSServer>>,
    mut read: impl StreamExt<Item = Result<Message, tungstenite::Error>> + Unpin,
    write: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    //ui: Arc<Mutex<AppState>>,
    pair_name: String,
) {
    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                if text.contains("\"op\":\"ping\"") {
                    let pong = "{\"op\":\"pong\"}";
                    let _ = write
                        .lock()
                        .await
                        .send(Message::Text(pong.into()))
                        .await;
                    continue;
                }

                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error parsing JSON: {} - skipping message", e);
                        continue;
                    }
                };

                let data = parsed.get("data").and_then(|data| data.get(0));

                if let Some(val) = data {
                    if let Some(price_str) = val.get("lastPr") {
                        if let Some(price_str) = price_str.as_str() {
                            if let Ok(price) = price_str.parse::<f64>() {
                                if let Some(ts) = val.get("ts") {
                                    if let Some(ts_str) = ts.as_str() {
                                        if let Ok(i64_ts) = ts_str.parse::<i64>() {
                                            BITGET_UPDATES_RECEIVED_COUNTER.inc();
                                            let safe_state = state.lock().await;
                                            safe_state
                                                .update_price(&pair_name, "bitget", price, i64_ts);

                                            if let Some(ref server_instance) = *server {
                                                server_instance.notify_price_change(
                                                    &safe_state.exchange_price_map,
                                                );
                                            }
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
            Ok(Message::Close(frame)) => {
                eprintln!("BITGET WebSocket closed: {:?}", frame);
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                // ignore pings/pongs for now
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

pub struct BitgetWSClient {}

impl ExchangeWSClient for BitgetWSClient {
    async fn subscribe(
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair_name: String,
    ) {
        let url = env::var("BITGET_WS_URL").expect("BITGET_WS_URL not set in .env");
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

        let (mut write, read) = ws_stream.split();

        let subscribe_msg = format!(
            r#"{{
                "op": "subscribe",
                "args": [
                    {{
                        "instType": "SPOT",
                        "channel": "ticker",
                        "instId": "{}"
                    }}
                ]
            }}"#,
            pair_name.to_uppercase().replace("-", "")
        );

        write
            .send(Message::Text(subscribe_msg.to_string().into()))
            .await
            .unwrap();

        let write = Arc::new(Mutex::new(write));

        // tokio::spawn(common::send_ping_loop(write, "Bitget"));

        let read_write_clone = Arc::clone(&write);

        tokio::spawn(handle_ws_read(
            state,
            server,
            read,
            read_write_clone,
            pair_name,
        ));
    }
}
