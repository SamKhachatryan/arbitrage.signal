use std::{sync::Arc, thread};

use crate::ws_server::WSServer;

pub fn init_ws_server() -> Arc<Option<WSServer>> {
    let server = Arc::new(Option::Some(WSServer::new()));

    let current_server = Arc::clone(&server);

    thread::spawn(move || {
        if let Some(ref server_instance) = *current_server {
            let event_hub = simple_websockets::launch(4010).expect("...");
            server_instance.start(event_hub);
        }
    });

    server
}