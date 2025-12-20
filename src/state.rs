use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc, offset::LocalResult};
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::Serialize;

use crate::common::order_book::OrderBook;

fn get_pairs_with_perps() -> Vec<String> {
    let mut pairs: Vec<String> = vec![
        // "btc-usdt",
        "eth-usdt",
        // "sol-usdt",
        "doge-usdt",
        // "xrp-usdt",
        // "ton-usdt",
        "ada-usdt",
        // "link-usdt",
        "arb-usdt",
        // "op-usdt",
        // "ltc-usdt",
        // "bch-usdt",
        // "uni-usdt",
        // "avax-usdt",
        "apt-usdt",
        // "near-usdt",
        // "matic-usdt",
        // "pepe-usdt",
        // "floki-usdt",
        // "sui-usdt",
        // "icp-usdt",
        // "xvs-usdt",
        "ach-usdt",
        // "fet-usdt",
        // "rndr-usdt",
        // "enj-usdt",
        // "mina-usdt",
        // "gala-usdt",
        "blur-usdt",
        // "wojak-usdt",
        // "bnb-usdt",
        // "cfx-usdt",
        // "kas-usdt",
        // "mon-usdt"
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
    pub order_book: OrderBook,
    pub latency: i32,
    pub last_update_ts: i64,
}

pub struct AppState {
    pub exchange_price_map: Arc<DashMap<String, DashMap<String, PairExchange>>>,
}

pub trait AppControl {
    fn update_order_book<F>(&self, pair: &str, exchange: &str, ts: i64, updater: F)
    where
        F: FnOnce(&mut OrderBook);
}

impl AppControl for AppState {
    fn update_order_book<F>(&self, pair: &str, exchange: &str, ts: i64, updater: F)
    where
        F: FnOnce(&mut OrderBook),
    {
        let now = Utc::now();
        if let LocalResult::Single(ts_datetime) = Utc.timestamp_millis_opt(ts) {
            let diff_ms = (now - ts_datetime).num_milliseconds() as i32;
            
            // Get or insert the exchange map for this pair
            let exchange_map = self.exchange_price_map
                .entry(pair.to_string())
                .or_insert_with(|| DashMap::new());
            
            let mut entry = exchange_map
                .entry(exchange.to_string())
                .or_insert_with(|| PairExchange {
                    order_book: OrderBook::new(),
                    latency: 0,
                    last_update_ts: 0,
                });
            
            updater(&mut entry.order_book);
            entry.latency = diff_ms.max(0);
            entry.last_update_ts = ts;
        }
    }
}

pub fn init_app_state() -> Arc<Mutex<AppState>> {
    let map: DashMap<String, DashMap<String, PairExchange>> = DashMap::new();

    for pair_name in PAIR_NAMES.iter() {
        map.insert(pair_name.to_string(), DashMap::new());
    }

    Arc::new(Mutex::new(AppState {
        exchange_price_map: Arc::new(map),
    }))
}
