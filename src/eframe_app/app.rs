use eframe::egui::{self, CentralPanel, ScrollArea};
use std::sync::{Arc, Mutex};

use crate::state::AppState;

// Shared application state that can be updated from other threads/tasks
// The `MyApp` struct is the eframe application wrapper which reads from shared state.
pub struct MyApp {
    pub state: Arc<Mutex<AppState>>,
}

impl MyApp {
    pub fn new(state: Arc<Mutex<AppState>>) -> Self {
        Self { state }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                // using scope to ensure lock release right after clone, so ws_clients want be blocked to write, waiting ui draw
                let cloned_data = {
                    let locked_state = self.state.lock().expect("Failed to lock");
                    let locked_prices = locked_state
                        .exchange_price_map
                        .lock()
                        .expect("Failed to lock");
                    locked_prices.clone()
                };

                let total = cloned_data.len();

                for (i, (key, value)) in cloned_data.iter().enumerate() {
                    ui.heading(key);

                    for (exchange_name, price) in value {
                        ui.label(format!("{}: {}", exchange_name, price));
                    }

                    let is_last = i + 1 >= total;

                    if !is_last {
                        ui.separator();
                    }
                }
            });
            ctx.request_repaint();
        });
    }
}
