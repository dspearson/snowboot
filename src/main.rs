// src/main.rs
//
// Snowboots for icy streams

mod icecast;
mod silence;
mod streamer;
mod validation;
mod util;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use clap::Parser;
use log::info;
use anyhow::Result;

use crate::streamer::OggStreamer;
use crate::validation::validators;
use crate::util::logging;
use crate::util::config;

static SERIAL_COUNTER: AtomicU32 = AtomicU32::new(1);
static GRANULE_COUNTER: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicBool = AtomicBool::new(true);

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "A tool to help with remuxing and streaming Ogg content over icecast")]
struct Args {
    /// Icecast server hostname (with optional port)
    #[arg(long, value_name = "HOST[:PORT]", help = "Icecast server address (e.g. 'example.com' or 'example.com:8000')")]
    host: String,

    /// Mount point on the Icecast server
    #[arg(long, value_name = "PATH", help = "Mount point path (e.g. '/stream.ogg')", value_parser = validators::validate_mount_point)]
    mount: String,

    /// Username for Icecast authentication
    #[arg(long, value_name = "USERNAME", help = "Username for server authentication")]
    user: String,

    /// Password for Icecast authentication
    #[arg(long, value_name = "PASSWORD", help = "Password for server authentication")]
    password: String,

    /// Path to the input pipe file
    #[arg(long, value_name = "PATH", help = "Path to the input pipe file", value_parser = validators::validate_input_pipe)]
    input_pipe: String,

    /// Logging level
    #[arg(long, value_name = "LEVEL", help = "Logging level (trace, debug, info, warn, error)", default_value = "info", value_parser = validators::validate_log_level)]
    log_level: String,

    /// Whether to continue after errors by sending silence
    #[arg(long, help = "Continue streaming with silence packets after input errors", default_value = "true")]
    keep_alive: bool,

    /// Maximum duration to stream silence before disconnecting (in seconds)
    #[arg(long, value_name = "SECONDS", help = "Maximum silence duration before disconnecting (0 = unlimited)", default_value = "0", value_parser = validators::validate_positive_number)]
    max_silence_duration: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Perform additional validation on arguments as a whole
    validators::validate_args(&args.host)?;

    // Initialize logging
    logging::setup(&args.log_level);

    // Extract host and port
    let (host, port) = config::parse_host_port(&args.host);

    info!("Starting snowboot v{}", env!("CARGO_PKG_VERSION"));
    info!("Connecting to {}:{}{}", host, port, args.mount);

    // Set up signal handling for graceful shutdown
    util::signals::setup_handlers();

    // Load silence data for keep-alive functionality
    let silence_data = util::silence::load_data(args.keep_alive).await?;

    // Create Icecast configuration
    let icecast_config = config::create_icecast_config(
        &args.host,
        &args.mount,
        &args.user,
        &args.password,
    );

    // Create a shared running flag
    let running = Arc::new(AtomicBool::new(true));

    // Set up the maximum silence duration
    let max_silence_duration = if args.max_silence_duration > 0 {
        Duration::from_secs(args.max_silence_duration)
    } else {
        Duration::from_secs(u64::MAX) // Effectively unlimited
    };

    // Create and run the streamer
    let mut streamer = OggStreamer::new(
        args.input_pipe,
        icecast_config,
        running.clone(),
        max_silence_duration,
        silence_data,
        args.keep_alive,
    );

    // Run the streamer
    match streamer.run().await {
        Ok(_) => info!("Stream completed successfully"),
        Err(e) => log::error!("Streaming error: {}", e),
    }

    info!("Shutting down");
    Ok(())
}
