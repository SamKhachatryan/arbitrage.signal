use std::sync::Arc;

use crate::{state::AppState, ws_server::WSServer};

pub fn init_ws_server(state: Arc<std::sync::Mutex<AppState>>) -> Arc<WSServer> {
    let server = WSServer::new();

    let state_clone = Arc::clone(&state);
    let server_clone = Arc::clone(&server);

    tokio::task::spawn_blocking(move || {
        let event_hub = simple_websockets::launch(4010).expect("Failed to launch websocket server");
        server_clone.start(state_clone, event_hub);
    });

    server
}