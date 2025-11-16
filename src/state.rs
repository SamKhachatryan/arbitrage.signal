use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{TimeZone, Utc, offset::LocalResult};
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::Serialize;

fn get_pairs_with_perps() -> Vec<String> {
    let mut pairs: Vec<String> = vec![
        "btc-usdt",
        "eth-usdt",
        "sol-usdt",
        "doge-usdt",
        "xrp-usdt",
        "ton-usdt",
        "ada-usdt",
        "link-usdt",
        "arb-usdt",
        "op-usdt",
        "ltc-usdt",
        "bch-usdt",
        "uni-usdt",
        "avax-usdt",
        "apt-usdt",
        "near-usdt",
        "matic-usdt",
        "pepe-usdt",
        "floki-usdt",
        "sui-usdt",
        "icp-usdt",
        "xvs-usdt",
        "ach-usdt",
        "fet-usdt",
        "rndr-usdt",
        "enj-usdt",
        "mina-usdt",
        "gala-usdt",
        "blur-usdt",
        "wojak-usdt",
        "bnb-usdt",
        "cfx-usdt",
        "kas-usdt",
    ]
    .iter()
    .map(|pair| pair.to_string())
    .collect();

    let mut perp_pairs: Vec<String> = pairs
        .iter()
        .map(|pair| format!("{}-perp", pair))
        .collect();

    pairs.append(&mut perp_pairs);
    pairs
}

lazy_static! {
    pub static ref PAIR_NAMES: Vec<String> = get_pairs_with_perps();
}

#[derive(Serialize, Clone)]
pub struct PairExchange {
    pub price: f64,
    pub latency: i32,
    pub last_update_ts: i64,
}

pub struct AppState {
    pub exchange_price_map: Arc<HashMap<String, DashMap<String, PairExchange>>>,
}

pub trait AppControl {
    fn update_price(&self, pair: &str, exchange: &str, price: f64, ts: i64);
}

impl AppControl for AppState {
    fn update_price(&self, pair: &str, exchange: &str, price: f64, ts: i64) {
        if let Some(exchange_map) = self.exchange_price_map.get(pair) {
            let now = Utc::now();
            if let LocalResult::Single(ts_datetime) = Utc.timestamp_millis_opt(ts) {
                let diff_ms = (now - ts_datetime).num_milliseconds() as i32;
                exchange_map.insert(
                    exchange.to_string(),
                    PairExchange {
                        price,
                        latency: diff_ms.max(0),
                        last_update_ts: ts,
                    },
                );
            }
        } else {
            eprintln!("Unknown pair: {}", pair);
        }
    }
}

pub fn init_app_state() -> Arc<Mutex<AppState>> {
    let mut map: HashMap<String, DashMap<String, PairExchange>> = HashMap::new();

    for pair_name in PAIR_NAMES.iter() {
        map.insert(pair_name.to_string(), DashMap::new());
    }

    Arc::new(Mutex::new(AppState {
        exchange_price_map: Arc::new(map),
    }))
}
