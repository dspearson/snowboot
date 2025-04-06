// src/util/logging.rs
//
// Logging configuration utilities

use log::LevelFilter;

/// Set up logging based on the specified log level
pub fn setup(log_level: &str) {
    env_logger::Builder::new()
        .filter_level(match log_level {
            "trace" => LevelFilter::Trace,
            "debug" => LevelFilter::Debug,
            "info" => LevelFilter::Info,
            "warn" => LevelFilter::Warn,
            "error" => LevelFilter::Error,
            _ => LevelFilter::Info,
        })
        .init();
}
