// src/util/signals.rs
//
// Signal handling utilities

use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Set up signal handlers for graceful shutdown
pub fn setup_handlers() {
    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        info!("Received shutdown signal, stopping stream...");
        r.store(false, Ordering::SeqCst);
        super::super::RUNNING.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
}
