use std::sync::{Arc, Mutex};

use futures::SinkExt;
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message;

use crate::{state::AppState, ws_server::WSServer};

pub trait ExchangeWSClient {
    async fn subscribe(state: Arc<Mutex<AppState>>, server: Arc<Option<WSServer>>, pair: String);
}

pub async fn send_ping_loop(mut write: impl SinkExt<Message> + Unpin) {
    let mut ticker = interval(Duration::from_secs(10));

    loop {
        ticker.tick().await;

        if let Err(_e) = write.send(Message::Ping(vec![].into())).await {
            eprintln!("Failed to send ping");
            break;
        }
    }
}
