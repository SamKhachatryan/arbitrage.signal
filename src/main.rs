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

#[tokio::main]
async fn main() {
    dotenv().ok().expect("Failed to load env variables");

    let state = Arc::new(Mutex::new(state::AppState {
        exchange_price_map: Arc::new(Mutex::new(HashMap::new())),
    }));

    ws_client::subscribe_to_all_exchanges(&state).await;

    let cloned_state = Arc::clone(&state);

    thread::spawn(move || {
        ws_server::init_server();
    });

    eframe_app::spawn_eframe_ui(cloned_state.clone());

    signal::ctrl_c()
        .await
        .expect("Failed to listen for shutdown signal");

    println!("Shutting down gracefully.");
}
