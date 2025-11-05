pub mod init;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;
use prometheus::{IntGauge, Opts};
use simple_websockets::{Event, EventHub, Message, Responder};

use crate::{define_prometheus_counter, health::prometheus::registry::METRIC_REGISTRY, state::PairExchange};

pub struct WSServer {
    clients: Arc<Mutex<HashMap<u64, Responder>>>,
}

define_prometheus_counter!(WS_SERVER_PACKAGES_SENT_COUNTER, "ws_server_packages_sent_counter", "WS Server: Total number of packages sent");

impl WSServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(&self, event_hub: EventHub) {
        let counter_opts = Opts::new("ws_server_clients_counter", "WS Server: Clients counter");
        let gauge: prometheus::core::GenericGauge<prometheus::core::AtomicI64> =
            IntGauge::with_opts(counter_opts).unwrap();

        let _ = METRIC_REGISTRY.register(Box::new(gauge.clone()));

        loop {
            match event_hub.poll_event() {
                Event::Connect(client_id, responder) => {
                    gauge.inc();
                    self.clients.lock().unwrap().insert(client_id, responder);
                }
                Event::Disconnect(client_id) => {
                    gauge.dec();
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
        exchange_price_map: &Arc<DashMap<String, HashMap<String, PairExchange>>>,
    ) {
        let resp_vect = {
            let map: HashMap<_, _> = exchange_price_map
                .iter()
                .map(|kv| (kv.key().clone(), kv.value().clone()))
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
