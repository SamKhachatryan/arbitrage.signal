pub mod init;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;
use simple_websockets::{Event, EventHub, Message, Responder};

use crate::{
    define_prometheus_counter, define_prometheus_gauge, state::PairExchange
};

pub struct WSServer {
    clients: Arc<Mutex<HashMap<u64, Responder>>>,
}

define_prometheus_gauge!(
    WS_SERVER_CLIENTS_GAUGE,
    "ws_server_clients_gauge",
    "WS Server: Clients gauge"
);

define_prometheus_counter!(
    WS_SERVER_PACKAGES_SENT_COUNTER,
    "ws_server_packages_sent_counter",
    "WS Server: Total number of packages sent"
);

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
                    WS_SERVER_CLIENTS_GAUGE.inc();
                    self.clients.lock().unwrap().insert(client_id, responder);
                }
                Event::Disconnect(client_id) => {
                    WS_SERVER_CLIENTS_GAUGE.dec();
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
        exchange_price_map: &Arc<HashMap<String, DashMap<String, PairExchange>>>,
    ) {
        let resp_vect = {
            let map: HashMap<_, _> = exchange_price_map
                .iter()
                .map(|(key, value)| {
                    let inner_map: HashMap<_, _> = value
                        .iter()
                        .map(|entry| (entry.key().clone(), entry.value().clone()))
                        .collect();
                    (key.clone(), inner_map)
                })
                .collect();

            // serde_json::to_string(&map).unwrap()
            rmp_serde::to_vec(&map).unwrap()
        };

        WS_SERVER_PACKAGES_SENT_COUNTER.inc();
        let clients = self.clients.lock().unwrap();
        for responder in clients.values() {
            responder.send(Message::Binary(resp_vect.clone()));
        }
    }
}
