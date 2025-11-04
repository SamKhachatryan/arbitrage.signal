use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{TimeZone, Utc, offset::LocalResult};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct PairExchange {
    pub price: f64,
    pub latency: i32,
    pub last_update_ts: i64,
}

pub struct AppState {
    pub exchange_price_map: Arc<Mutex<HashMap<String, HashMap<String, PairExchange>>>>,
}

pub trait AppControl {
    fn update_price(&mut self, pair: &str, exchange: &str, price: f64, ts: i64);
}

impl AppControl for AppState {
    fn update_price(&mut self, pair: &str, exchange: &str, price: f64, ts: i64) {
        let mut locked_exchange = self
            .exchange_price_map
            .lock()
            .expect("Failed to update price");
        let exchange_map = locked_exchange
            .entry(pair.to_string())
            .or_insert_with(HashMap::new);

        let now = Utc::now();
        let ts_datetime_result = Utc.timestamp_millis_opt(ts);

        if let LocalResult::Single(ts_datetime) = ts_datetime_result {
            let diff_ms = (now - ts_datetime).num_milliseconds() as i32;
            exchange_map.insert(
                exchange.to_string(),
                PairExchange {
                    price: price,
                    latency: diff_ms,
                    last_update_ts: ts,
                },
            );
        }
    }
}

pub fn init_app_state() -> Arc<Mutex<AppState>> {
    Arc::new(Mutex::new(AppState {
        exchange_price_map: Arc::new(Mutex::new(HashMap::new())),
    }))
}