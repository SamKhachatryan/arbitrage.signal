// mod app;

// use std::sync::{Arc, Mutex};

// use eframe::NativeOptions;

// use crate::{eframe_app::app::MyApp, state::AppState};

// pub fn spawn_eframe_ui(app: Arc<Mutex<AppState>>) {
//     let options = NativeOptions::default();

//     // eframe expects a closure that returns a boxed App
//     let _ = eframe::run_native(
//         "Live Pair-Exchange prices",
//         options,
//         Box::new(|_cc| {
//             // eframe app will hold a clone of the shared state
//             Ok(Box::new(MyApp::new(app)))
//         }),
//     );
// }