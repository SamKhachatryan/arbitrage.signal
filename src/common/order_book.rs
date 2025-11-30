use std::{cmp::min, collections::BTreeMap};

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

    pub fn get_best_bid(&self) -> Option<(&String, &f64)> {
        self.bids
            .iter()
            .max_by(|a, b| a.0.parse::<f64>().unwrap().partial_cmp(&b.0.parse::<f64>().unwrap()).unwrap())
    }

    pub fn get_best_ask(&self) -> Option<(&String, &f64)> {
        self.asks
            .iter()
            .min_by(|a, b| a.0.parse::<f64>().unwrap().partial_cmp(&b.0.parse::<f64>().unwrap()).unwrap())
    }

    pub fn get_mid_price(&self) -> Option<f64> {
        if let (Some((best_bid, _)), Some((best_ask, _))) = (self.get_best_bid(), self.get_best_ask()) {
            Some((best_bid.parse::<f64>().unwrap() + best_ask.parse::<f64>().unwrap()) / 2.0)
        } else {
            None
        }
    }
}
