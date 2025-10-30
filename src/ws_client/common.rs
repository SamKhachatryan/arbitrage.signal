use std::sync::{Arc, Mutex};

use crate::state::AppState;

pub trait ExchangeWSClient {
    async fn subscribe(state: Arc<Mutex<AppState>>, pair: String);
}