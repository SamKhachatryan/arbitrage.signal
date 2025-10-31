mod eframe_app;
mod state;
mod ws_client;
mod ws_server;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
};

use dotenvy::dotenv;
use tokio::signal;

use crate::ws_server::WSServer;

#[tokio::main]
async fn main() {
    dotenv().ok().expect("Failed to load env variables");

    let state = Arc::new(Mutex::new(state::AppState {
        exchange_price_map: Arc::new(Mutex::new(HashMap::new())),
    }));

    let cloned_state = Arc::clone(&state);

    let server = Arc::new(Option::Some(WSServer::new()));

    let current_server = Arc::clone(&server);

    thread::spawn(move || {
        if let Some(ref server_instance) = *current_server {
            let event_hub = simple_websockets::launch(4010).expect("...");
            server_instance.start(event_hub);
        }
    });

    let client_server = Arc::clone(&server);

    ws_client::subscribe_to_all_exchanges(&state, client_server).await;

    eframe_app::spawn_eframe_ui(cloned_state.clone());

    signal::ctrl_c()
        .await
        .expect("Failed to listen for shutdown signal");

    println!("Shutting down gracefully.");
}
