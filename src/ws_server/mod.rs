pub mod init;

use std::{collections::HashMap, sync::{Arc, Weak}};

use dashmap::DashMap;
use simple_websockets::{Event, EventHub, Message, Responder};
use tokio_util::sync::CancellationToken;

use crate::{
    define_prometheus_counter, define_prometheus_gauge,
    exchanges::subscribe_to_ticker,
    state::{AppState, PairExchange},
};

const MAX_TOPICS: usize = 20;

pub struct WSServer {
    clients: Arc<DashMap<u64, Responder>>,
    topics: Arc<DashMap<String, Vec<u64>>>,
    client_topics: Arc<DashMap<String, Vec<String>>>,
    topic_tasks: Arc<DashMap<String, CancellationToken>>,
    self_arc: Weak<WSServer>,
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
    pub fn new() -> Arc<Self> {
        let server = Arc::new_cyclic(|weak| Self {
            clients: Arc::new(DashMap::new()),
            topics: Arc::new(DashMap::new()),
            client_topics: Arc::new(DashMap::new()),
            topic_tasks: Arc::new(DashMap::new()),
            self_arc: weak.clone(),
        });
        server
    }

    pub fn start(&self, state: Arc<std::sync::Mutex<AppState>>, event_hub: EventHub) {
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
                            let should_cleanup = {
                                if let Some(mut entry) = self.topics.get_mut(&topic) {
                                    entry.retain(|&id| id != client_id);
                                    entry.is_empty()
                                } else {
                                    false
                                }
                            };

                            println!("{}", should_cleanup);
                            
                            if should_cleanup {
                                self.topics.remove(&topic);
                                println!("Removed topic: {} (remaining topics: {})", topic, self.topics.len());
                                
                                if let Some((_, cancel_token)) = self.topic_tasks.remove(&topic) {
                                    cancel_token.cancel();
                                    println!("Cancelled task for topic: {}", topic);
                                }
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

                                    // Check topic limit before acquiring any locks
                                    let is_new_topic = !self.topics.contains_key(&topic);
                                    let should_reject = is_new_topic && self.topics.len() >= MAX_TOPICS;
                                    
                                    if should_reject {
                                        println!("Max topics limit reached ({}/{}), rejecting subscription to: {}", self.topics.len(), MAX_TOPICS, topic);
                                    } else {
                                        match self.topics.entry(topic.clone()) {
                                            dashmap::mapref::entry::Entry::Occupied(mut occ) => {
                                                let entry = occ.get_mut();
                                                if !entry.contains(&client_id) {
                                                    entry.push(client_id);
                                                } else {
                                                }
                                            }
                                            dashmap::mapref::entry::Entry::Vacant(vac) => {
                                                vac.insert(vec![client_id]);
                                                println!("Created NEW topic: {} (total topics now: {})", topic, self.topics.len() + 1);

                                                // Create cancellation token and spawn the subscription task
                                                if let Some(self_arc) = self.self_arc.upgrade() {
                                                    let state_clone = Arc::clone(&state);
                                                    let cancel_token = CancellationToken::new();
                                                    let cancel_token_clone = cancel_token.clone();
                                                    
                                                    tokio::spawn(subscribe_to_ticker(
                                                        topic.clone(),
                                                        state_clone,
                                                        self_arc,
                                                        cancel_token_clone,
                                                    ));
                                                    
                                                    self.topic_tasks.insert(topic.clone(), cancel_token);
                                                    println!("Spawned task for NEW topic: {} with client {}", topic, client_id);
                                                }
                                            }
                                        }
                                        
                                        self.client_topics
                                            .entry(client_id.to_string())
                                            .or_insert_with(Vec::new)
                                            .push(topic.clone());
                                    }
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
        exchange_price_map: &Arc<DashMap<String, DashMap<String, PairExchange>>>,
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
                            // Only notify if the topic exists (don't create it)
                            if let Some(clients) = self.topics.get(pair_name) {
                                for client_id in clients.iter() {
                                    match self.clients.get(client_id) {
                                        Some(r) => {
                                            r.send(Message::Binary(resp_vect.clone()));
                                        }
                                        None => continue,
                                    };
                                }
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
