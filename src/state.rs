use std::{collections::HashMap, sync::{Arc, Mutex}};

pub struct AppState {
    pub exchange_price_map: Arc<Mutex<HashMap<String, HashMap<String, f64>>>>,
}

pub trait AppControl {
    fn update_price(&mut self, pair: &str, exchange: &str, price: f64);
}

impl AppControl for AppState {
    fn update_price(&mut self, pair: &str, exchange: &str, price: f64) {
        let mut locked_exchange = self.exchange_price_map.lock().expect("Failed to update price");
        let exchange_map = locked_exchange.entry(pair.to_string()).or_insert_with(HashMap::new);
        exchange_map.insert(exchange.to_string(), price);
    }
}