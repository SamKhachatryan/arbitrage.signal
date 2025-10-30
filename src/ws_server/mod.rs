use simple_websockets::{Event, EventHub, Responder};
use std::{collections::HashMap, sync::{Arc, Mutex}};

pub struct WSServer {
    clients: HashMap<u64, Responder>,
    event_hub: EventHub,
}

pub trait WSServerNotification {
    fn notify_price_change(exchange_price_map: Arc<Mutex<HashMap<String, HashMap<String, f64>>>>);
}

impl WSServerNotification for WSServer {
    fn notify_price_change(exchange_price_map: Arc<Mutex<HashMap<String, HashMap<String, f64>>>>) {
        // releasing lock earlier
        let json = {
            let safe_exchange_price_map = {
                let locked  = exchange_price_map.lock().expect("Failed to lock");
                locked.clone()
            };
            serde_json::to_string(&safe_exchange_price_map).unwrap();
        };

    }
}

pub fn init_server() {
    let mut ws_server = WSServer {
        event_hub: simple_websockets::launch(4010).expect("ws_server: failed to listen on port 4010"),
        clients: HashMap::new(),
    };

    loop {
        match ws_server.event_hub.poll_event() {
            Event::Connect(client_id, responder) => {
                println!("A client connected with id #{}", client_id);
                // add their Responder to our `clients` map:
                ws_server.clients.insert(client_id, responder);
            },
            Event::Disconnect(client_id) => {
                println!("Client #{} disconnected.", client_id);
                // remove the disconnected client from the clients map:
                ws_server.clients.remove(&client_id);
            },
            Event::Message(client_id, message) => {
                println!("Received a message from client #{}: {:?}", client_id, message);
                // retrieve this client's `Responder`:
                let responder = ws_server.clients.get(&client_id).unwrap();
                // echo the message back:
                responder.send(message);
            },
        }
    }
}