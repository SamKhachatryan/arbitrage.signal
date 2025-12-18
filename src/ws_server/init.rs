use std::{sync::Arc, thread};

use crate::ws_server::WSServer;

pub fn init_ws_server() -> Arc<WSServer> {
    let server = Arc::new(WSServer::new());

    let server_clone = Arc::clone(&server);

    thread::spawn(move || {
        let event_hub = simple_websockets::launch(4010).expect("Failed to launch websocket server");
        server_clone.start(event_hub);
    });

    server
}