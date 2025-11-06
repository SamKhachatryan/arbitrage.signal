use std::sync::{Arc};

use futures::SinkExt;
use tokio::{sync::Mutex, time::{Duration, interval}};
use tokio_tungstenite::tungstenite::Message;

use crate::{state::AppState, ws_server::WSServer};

pub trait ExchangeWSClient {
    async fn subscribe(state: Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>, pair: String);
}

pub trait ExchangeWSClientPairList {
    async fn subscribe_list(state: Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>, pairs: Vec<String>);
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
