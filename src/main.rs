// src/main.rs
//
// Snowboots for icy streams

mod icecast;
mod silence;
mod streamer;

use std::sync::atomic::AtomicU32;
use std::sync::atomic::AtomicU64;
use std::os::unix::fs::FileTypeExt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use clap::{Parser, Subcommand};
use log::{info, warn, error, debug, trace};
use anyhow::{Result, Context};

use crate::icecast::IcecastConfig;
use crate::streamer::OggStreamer;

static SERIAL_COUNTER: AtomicU32 = AtomicU32::new(1);
static GRANULE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "A tool to help with remuxing and streaming Ogg content over icecast")]
struct Args {
    /// Icecast server hostname (with optional port)
    #[arg(long, value_name = "HOST[:PORT]", help = "Icecast server address (e.g. 'example.com' or 'example.com:8000')")]
    host: String,

    /// Mount point on the Icecast server
    #[arg(long, value_name = "PATH", help = "Mount point path (e.g. '/stream.ogg')", value_parser = validate_mount_point)]
    mount: String,

    /// Username for Icecast authentication
    #[arg(long, value_name = "USERNAME", help = "Username for server authentication")]
    user: String,

    /// Password for Icecast authentication
    #[arg(long, value_name = "PASSWORD", help = "Password for server authentication")]
    password: String,

    /// Path to the input pipe file
    #[arg(long, value_name = "PATH", help = "Path to the input pipe file", value_parser = validate_input_pipe)]
    input_pipe: String,

    /// Logging level
    #[arg(long, value_name = "LEVEL", help = "Logging level (trace, debug, info, warn, error)", default_value = "info", value_parser = validate_log_level)]
    log_level: String,

    /// Whether to continue after errors by sending silence
    #[arg(long, help = "Continue streaming with silence packets after input errors", default_value = "true")]
    keep_alive: bool,

    /// Maximum duration to stream silence before disconnecting (in seconds)
    #[arg(long, value_name = "SECONDS", help = "Maximum silence duration before disconnecting (0 = unlimited)", default_value = "0", value_parser = validate_positive_number)]
    max_silence_duration: u64,

    /// Stream name to pass to Icecast
    #[arg(long, value_name = "NAME", help = "Stream name to display in Icecast")]
    stream_name: Option<String>,

    /// Stream description to pass to Icecast
    #[arg(long, value_name = "DESCRIPTION", help = "Stream description for Icecast")]
    stream_description: Option<String>,

    /// Stream genre to pass to Icecast
    #[arg(long, value_name = "GENRE", help = "Stream genre for Icecast")]
    stream_genre: Option<String>,

    /// Stream URL to pass to Icecast
    #[arg(long, value_name = "URL", help = "Stream URL for Icecast")]
    stream_url: Option<String>,

    /// Whether the stream should be listed as public
    #[arg(long, help = "List the stream as public on directory servers")]
    public: bool,
}

// Validation functions
fn validate_mount_point(s: &str) -> Result<String, String> {
    if !s.starts_with('/') {
        return Err("Mount point must start with a '/'".to_string());
    }
    Ok(s.to_string())
}

fn validate_input_pipe(s: &str) -> Result<String, String> {
    let path = Path::new(s);
    if path.exists() {
        // Check if it's a pipe or regular file
        match path.metadata() {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                if !(file_type.is_fifo() || file_type.is_file()) {
                    return Err("Input must be a pipe (FIFO) or regular file".to_string());
                }
            },
            Err(e) => return Err(format!("Cannot access input pipe: {}", e)),
        }
    }
    Ok(s.to_string())
}

fn validate_log_level(s: &str) -> Result<String, String> {
    match s {
        "trace" | "debug" | "info" | "warn" | "error" => Ok(s.to_string()),
        _ => Err("Log level must be one of: trace, debug, info, warn, error".to_string()),
    }
}

fn validate_positive_number(s: &str) -> Result<u64, String> {
    match s.parse::<u64>() {
        Ok(n) => Ok(n),
        Err(_) => Err("Value must be a positive number".to_string()),
    }
}

impl Args {
    /// Validate all arguments together after individual validation
    fn validate(&self) -> Result<(), String> {
        // Check host format
        let host_parts: Vec<&str> = self.host.split(':').collect();
        if host_parts.len() > 2 {
            return Err("Host format should be 'hostname' or 'hostname:port'".to_string());
        }

        if host_parts.len() == 2 {
            if let Err(_) = host_parts[1].parse::<u16>() {
                return Err("Port must be a valid number between 1-65535".to_string());
            }
        }

        Ok(())
    }

    /// Get the host and port, defaulting to port 8000 if not specified
    fn get_host_and_port(&self) -> (String, u16) {
        let parts: Vec<&str> = self.host.split(':').collect();
        let host = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().unwrap_or(8000)
        } else {
            8000 // Default Icecast port
        };

        (host, port)
    }
}

// Global running flag for signal handling
static RUNNING: AtomicBool = AtomicBool::new(true);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Perform additional validation on arguments as a whole
    args.validate().context("Invalid command line arguments")?;

    // Extract host and port
    let (host, port) = args.get_host_and_port();
    debug!("Using server {}:{}", host, port);

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

    info!("Starting snowboot v{}", env!("CARGO_PKG_VERSION"));
    info!("Connecting to {}:{}{}", host, port, args.mount);

    // Set up signal handling for graceful shutdown
    setup_signal_handlers();

    // Load silence data for keep-alive functionality
    let silence_data = if args.keep_alive {
        match silence::load_embedded_silence() {
            Ok(data) => {
                info!("Loaded {} silence packets ({} bytes)",
                      data.packets.len(),
                      data.total_size);
                Some(data)
            },
            Err(e) => {
                error!("Failed to load silence data: {}", e);
                warn!("Keep-alive functionality will be disabled");
                None
            }
        }
    } else {
        debug!("Keep-alive functionality is disabled");
        None
    };

    // Create Icecast configuration
    let icecast_config = IcecastConfig {
        host,
        port
        mount: args.mount.clone(),
        username: args.user.clone(),
        password: args.password.clone(),
        content_type: "application/ogg".to_string(),
        name: args.stream_name,
        description: args.stream_description,
        genre: args.stream_genre,
        url: args.stream_url,
        is_public: Some(args.public),
    };

    // Create a shared running flag
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

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
        running_clone,
        max_silence_duration,
        silence_data,
        args.keep_alive,
    );

    // Run the streamer
    match streamer.run().await {
        Ok(_) => info!("Stream completed successfully"),
        Err(e) => error!("Streaming error: {:?}", e),
    }

    info!("Shutting down");
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
