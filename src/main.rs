// src/main.rs
//
// Snowboots for icy streams

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use clap::{Parser, Subcommand};
use log::{info, warn, error, debug, trace};
use tokio::time::sleep;
use ogg::{PacketReader, PacketWriter, Packet};
use ogg::writing::PacketWriteEndInfo;

mod ogg;
mod silence;

const SILENCE_INTERVAL_MS: u64 = 500; // Send silence every 500ms when no input
static SILENCE_DATA: &[u8] = include_bytes!("../resources/silence.ogg");
static SERIAL_COUNTER: AtomicU32 = AtomicU32::new(1);
static GRANULE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(long)] host: String,
    #[arg(long)] mount: String,
    #[arg(long)] user: String,
    #[arg(long)] password: String,
    #[arg(long)] input_pipe: String,
}

// Global running flag for signal handling
static RUNNING: AtomicBool = AtomicBool::new(true);

/// Preloaded silence data
pub struct SilenceData {
    packets: Vec<(Vec<u8>, i64)>,  // (packet_data, granule_increment)
    total_size: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    env_logger::Builder::new()
        .filter_level(match args.log_level.as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Info,
        })
        .init();

    // Set up signal handling for graceful shutdown
    setup_signal_handlers();

    Ok(())
}

fn setup_signal_handlers() {
    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        info!("Received shutdown signal, stopping stream...");
        r.store(false, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");
}
