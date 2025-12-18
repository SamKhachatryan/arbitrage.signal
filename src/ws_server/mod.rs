pub mod init;

use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use simple_websockets::{Event, EventHub, Message, Responder};

use crate::{define_prometheus_counter, define_prometheus_gauge, state::PairExchange};

pub struct WSServer {
    clients: Arc<DashMap<u64, Responder>>,
    topics: Arc<DashMap<String, Vec<u64>>>,
    client_topics: Arc<DashMap<String, Vec<String>>>,
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

#[derive(serde::Deserialize)]
struct SubscribeTopic {
    topic: String,
}

impl WSServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            topics: Arc::new(DashMap::new()),
            client_topics: Arc::new(DashMap::new()),
        }
    }

    pub fn start(&self, event_hub: EventHub) {
        loop {
            match event_hub.poll_event() {
                Event::Connect(client_id, responder) => {
                    WS_SERVER_CLIENTS_GAUGE.inc();
                    self.clients.insert(client_id, responder);
                }
                Event::Disconnect(client_id) => {
                    WS_SERVER_CLIENTS_GAUGE.dec();
                    self.clients.remove(&client_id);
                    if let Some(topics) = self.client_topics.remove(&client_id.to_string()) {
                        for topic in topics.1 {
                            if let Some(mut entry) = self.topics.get_mut(&topic) {
                                entry.retain(|&id| id != client_id);
                            }
                        }
                    }
                }
                Event::Message(client_id, msg) => {
                    match msg {
                        Message::Text(text) => {
                            // Deserialize JSON into your struct
                            match serde_json::from_str::<SubscribeTopic>(&text) {
                                Ok(parsed) => {
                                    let topic = parsed.topic;
                                    self.topics
                                        .entry(topic.clone())
                                        .or_insert_with(Vec::new)
                                        .push(client_id);

                                    self.client_topics
                                        .entry(client_id.to_string())
                                        .or_insert_with(Vec::new)
                                        .push(topic);
                                }
                                Err(e) => {
                                    println!("Failed to parse message from {}: {}", client_id, e);
                                }
                            }
                        }
                        Message::Binary(bin) => {
                            println!("Client {} sent binary data: {:?}", client_id, bin);
                            // You could also deserialize binary formats here (e.g. bincode)
                        }
                    }
                }
            }
        }
    }

    pub fn notify_price_change(
        &self,
        exchange_price_map: &Arc<HashMap<String, DashMap<String, PairExchange>>>,
        pair_name: &str,
        exchange_name: &str,
    ) {
        WS_SERVER_PACKAGES_SENT_COUNTER.inc();

        match exchange_price_map.get(pair_name) {
            Some(exchange_map) => match exchange_map.get(exchange_name) {
                Some(pair_exchange) => {
                    let mut map = HashMap::new();
                    map.insert(pair_name.to_string(), {
                        let mut inner_map = HashMap::new();
                        inner_map.insert(exchange_name.to_string(), pair_exchange.clone());
                        inner_map
                    });

                    match rmp_serde::to_vec(&map) {
                        Ok(resp_vect) => {
                            let clients = self.topics.entry(pair_name.to_string()).or_default();
                            for client_id in clients.iter() {
                                match self.clients.get(client_id) {
                                    Some(r) => {
                                        r.send(Message::Binary(resp_vect.clone()));
                                    }
                                    None => continue,
                                };
                            }
                        }
                        Err(e) => {
                            eprintln!("Serialization failed: {}, {}", e, pair_name);
                        }
                    }
                }
                None => return,
            },
            None => return,
        }
    }
}
