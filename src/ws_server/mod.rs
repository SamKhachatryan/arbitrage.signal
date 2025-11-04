pub mod init;

use std::{collections::HashMap, sync::{Arc, Mutex}};

use lazy_static::lazy_static;
use prometheus::{Counter, IntGauge, Opts};
use simple_websockets::{Event, EventHub, Message, Responder};

use crate::{health::prometheus::METRIC_REGISTRY, state::PairExchange};

pub struct WSServer {
    clients: Arc<Mutex<HashMap<u64, Responder>>>,
}

lazy_static! {
    pub static ref WS_SERVER_PACKAGES_SENT_COUNTER: Counter = {
        let opts = Opts::new("ws_server_packages_sent_counter", "WS Server: Total number of packages sent");
        let counter = Counter::with_opts(opts).expect("Failed to create counter");
        METRIC_REGISTRY.register(Box::new(counter.clone())).expect("Failed to register counter");
        counter
    };

}

impl WSServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(&self, event_hub: EventHub) {
        let counter_opts = Opts::new("ws_server_clients_counter", "WS Server: Clients counter");
        let gauge: prometheus::core::GenericGauge<prometheus::core::AtomicI64> = IntGauge::with_opts(counter_opts).unwrap();

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
        exchange_price_map: &Arc<Mutex<HashMap<String, HashMap<String, PairExchange>>>>,
    ) {
        let json = {
            let map = exchange_price_map.lock().unwrap();
            serde_json::to_string(&*map).unwrap()
        };

        WS_SERVER_PACKAGES_SENT_COUNTER.inc();
        let clients = self.clients.lock().unwrap();
        for responder in clients.values() {
            responder.send(Message::Text(json.clone()));
        }
    }
}