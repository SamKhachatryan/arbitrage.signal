use std::{cmp::min, collections::BTreeMap};

use rust_decimal::{Decimal, prelude::FromPrimitive, prelude::ToPrimitive};
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

    pub fn clean_bids(&mut self) {
        self.bids.clear();
    }

    pub fn clean_asks(&mut self) {
        self.asks.clear();
    }

    pub fn update_bid(&mut self, price: f64, quantity: f64) {
        if quantity == 0.0 {
            self.bids.remove(&price.to_string());
        } else {
            let notional_amount = Decimal::from_f64(quantity)
                .and_then(|a| Decimal::from_f64(price).and_then(|b| Some(a * b)))
                .and_then(|r| r.to_f64());

            if let Some(notional_amount) = notional_amount {
                self.bids
                    .insert(price.to_string(), notional_amount);
            }
        }
    }

    pub fn get_depth(&self) -> usize {
        min(self.bids.len(), self.asks.len())
    }

    pub fn update_ask(&mut self, price: f64, quantity: f64) {
        if quantity == 0.0 {
            self.asks.remove(&price.to_string());
        } else {
            let notional_amount = Decimal::from_f64(quantity)
                .and_then(|a| Decimal::from_f64(price).and_then(|b| Some(a * b)))
                .and_then(|r| r.to_f64());

            if let Some(notional_amount) = notional_amount {
                self.asks
                    .insert(price.to_string(), notional_amount);
            }
        }
    }
}
