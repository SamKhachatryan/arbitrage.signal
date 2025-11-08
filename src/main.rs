mod eframe_app;
mod health;
mod state;
mod ws_client;
mod ws_server;
mod tls;

use std::sync::Arc;

use dotenvy::dotenv;
use tokio::signal;

use crate::{health::prometheus::init_prometheus_server, state::init_app_state, ws_server::init::init_ws_server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok().expect("Failed to load env variables");

    let server = init_ws_server();
    let client_server = Arc::clone(&server);

    let state = init_app_state();

    ws_client::subscribe_to_all_exchanges(&state, client_server).await;

    let http_server = init_prometheus_server();

    tokio::select! {
        _ = http_server => {
            println!("HTTP Server stopped");
        }
        _ = signal::ctrl_c() => {
            println!("Shutting down");
        }
    }

    Ok(())
}
