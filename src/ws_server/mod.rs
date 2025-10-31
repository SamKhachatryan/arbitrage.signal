use std::{collections::HashMap, sync::{Arc, Mutex}};

use simple_websockets::{Event, EventHub, Message, Responder};

pub struct WSServer {
    clients: Arc<Mutex<HashMap<u64, Responder>>>,
}

impl WSServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(&self, event_hub: EventHub) {
        loop {
            match event_hub.poll_event() {
                Event::Connect(client_id, responder) => {
                    self.clients.lock().unwrap().insert(client_id, responder);
                }
                Event::Disconnect(client_id) => {
                    self.clients.lock().unwrap().remove(&client_id);
                }
                Event::Message(client_id, msg) => {
                    if let Some(responder) = self.clients.lock().unwrap().get(&client_id) {
                        responder.send(msg);
                    }
                }
            }
        }
    }

    pub fn notify_price_change(
        &self,
        exchange_price_map: &Arc<Mutex<HashMap<String, HashMap<String, f64>>>>,
    ) {
        let json = {
            let map = exchange_price_map.lock().unwrap();
            serde_json::to_string(&*map).unwrap()
        };

        let clients = self.clients.lock().unwrap();
        for responder in clients.values() {
            responder.send(Message::Text(json.clone()));
        }
    }
}