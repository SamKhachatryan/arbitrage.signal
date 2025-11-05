use prometheus::{Registry};

use lazy_static::lazy_static;

lazy_static! {
    pub static ref METRIC_REGISTRY: Registry = Registry::new();
}