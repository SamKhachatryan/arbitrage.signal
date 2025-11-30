use std::sync::Arc;

use futures::SinkExt;
use tokio::{
    sync::Mutex, time::{Duration, interval}
};
use tokio_tungstenite::tungstenite::Message;

pub async fn send_ping_loop(
    write: Arc<Mutex<impl SinkExt<Message> + Unpin>>,
    exchange_name: &str,
) -> Result<(), ()> {
    let mut ticker = interval(Duration::from_secs(10));

    loop {
        ticker.tick().await;

        if let Err(_e) = write.lock().await.send(Message::Ping(vec![].into())).await {
            eprintln!("[{}] Failed to send ping, connection likely closed", exchange_name);
            return Err(());
        }
    }
}


