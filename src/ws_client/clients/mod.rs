pub mod binance;
pub mod okx;
pub mod gate;
pub mod bybit;
pub mod whitebit;
pub mod bitget;
// pub mod crypto;
pub mod interface;

use crate::define_prometheus_counter;

define_prometheus_counter!(
    WS_CLIENTS_PACKAGES_RECEIVED_COUNTER,
    "ws_clients_packages_received_counter",
    "WS Clients: Total number of packages received"
);