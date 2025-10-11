// Utility module for signal handling

use tracing::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Set up signal handlers for graceful shutdown
pub fn setup_signal_handlers(running: Arc<AtomicBool>) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        info!("Received shutdown signal, stopping stream...");
        running.store(false, Ordering::SeqCst);
    })
}
