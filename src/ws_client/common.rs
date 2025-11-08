use std::sync::{Arc};

use futures::SinkExt;
use tokio::{sync::Mutex, time::{Duration, interval}};
use tokio_tungstenite::tungstenite::Message;

use crate::{state::AppState, ws_server::WSServer};

pub trait ExchangeWSClient {
    async fn subscribe(state: Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>, pair: String);
    // async fn subscribe_list(state: Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>, pairs: Vec<String>);
}

pub async fn send_ping_loop(mut write: impl SinkExt<Message> + Unpin, exchange_name: &str) {
    let mut ticker = interval(Duration::from_secs(10));

    loop {
        ticker.tick().await;

        if let Err(_e) = write.send(Message::Ping(vec![].into())).await {
            eprintln!("Failed to send ping: {}", exchange_name);
            break;
        }
    }
}

pub async fn send_ping_loop_async(mut write: Arc<Mutex<impl SinkExt<Message> + Unpin>>, exchange_name: &str) {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        if let Err(_e) = write.lock().await.send(Message::Ping(vec![].into())).await {
            eprintln!("Failed to send ping: {}", exchange_name);
            break;
        }
    }
}