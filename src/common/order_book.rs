use std::{cmp::min, collections::BTreeMap};

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct OrderBook {
    bids: BTreeMap<String, f64>, // price -> quantity
    asks: BTreeMap<String, f64>, // price -> quantity
}

impl OrderBook {
    pub fn new() -> Self {
        OrderBook {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn update_bid(&mut self, price: f64, quantity: f64) {
        if quantity == 0.0 {
            self.bids.remove(&price.to_string());
        } else {
            self.bids.insert(price.to_string(), quantity);
        }
    }

    pub fn get_depth(&self) -> usize {
        min(self.bids.len(), self.asks.len())
    }

    pub fn update_ask(&mut self, price: f64, quantity: f64) {
        if quantity == 0.0 {
            self.asks.remove(&price.to_string());
        } else {
            self.asks.insert(price.to_string(), quantity);
        }
    }
}
