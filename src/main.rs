// src/main.rs
//
// Snowboots for icy streams - simplified implementation using oggmux

mod config;
mod connection;
mod errors;
mod icecast;
mod metrics;
mod server;
mod util;
mod validation;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::errors::Result;
use bytes::Bytes;
use clap::Parser;
use tracing::{debug, error, info, warn, instrument};
use oggmux::{OggMux, VorbisConfig, VorbisBitrateMode, BufferConfig};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

use crate::icecast::{IcecastClient, IcecastConfig};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "A tool to help with remuxing and streaming Ogg content over Icecast")]
struct Args {
    /// Icecast server hostname (with optional port)
    #[arg(long, value_name = "HOST[:PORT]", default_value = "localhost:8000", help = "Icecast server address (e.g. 'example.com' or 'example.com:8000')")]
    host: String,

    /// Mount point on the Icecast server
    #[arg(long, value_name = "PATH", default_value = "/stream.ogg", help = "Mount point path (e.g. '/stream.ogg')")]
    mount: String,

    /// Username for Icecast authentication
    #[arg(long, value_name = "USERNAME", default_value = "source", help = "Username for server authentication")]
    user: String,

    /// Password for Icecast authentication
    #[arg(long, value_name = "PASSWORD", default_value = "hackme", help = "Password for server authentication")]
    password: String,

    /// Path to the input pipe file
    #[arg(long, value_name = "PATH", default_value = "/tmp/snowboot.in", help = "Path to the input pipe file")]
    input_pipe: String,

    /// Sample rate for the Ogg Vorbis stream
    #[arg(long, value_name = "RATE", default_value = "44100", help = "Sample rate in Hz (e.g., 44100, 48000)")]
    sample_rate: u32,

    /// Bitrate for the Ogg Vorbis stream
    #[arg(long, value_name = "BITRATE", default_value = "320", help = "Bitrate in kbps (e.g., 128, 192, 320)")]
    bitrate: u32,

    /// Buffer size in seconds
    #[arg(long, value_name = "SECONDS", default_value = "1.0", help = "Buffer size in seconds")]
    buffer: f64,

    /// Log level
    #[arg(long, value_name = "LEVEL", default_value = "info",
          help = "Log level (trace, debug, info, warn, error)")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging (default to text format for now, can be made configurable)
    setup_logging(&args.log_level, "text");

    // Parse host and port
    let (host, port) = parse_host_port(&args.host)?;

    info!("Starting snowboot v{}", env!("CARGO_PKG_VERSION"));
    info!("Connecting to {}:{}{}", host, port, args.mount);

    // Set up the running flag and signal handlers
    let running = Arc::new(AtomicBool::new(true));
    setup_signal_handlers(running.clone())?;

    // Configure the Icecast client
    let icecast_config = IcecastConfig {
        host,
        port,
        mount: args.mount,
        username: args.user,
        password: args.password,
        content_type: "application/ogg".to_string(),
    };

    let icecast_client = IcecastClient::new(icecast_config);

    // Connect to Icecast
    icecast_client.connect().await?;

    // Configure and spawn the OggMux
    let mux = OggMux::new()
        .with_vorbis_config(VorbisConfig {
            sample_rate: args.sample_rate,
            bitrate: VorbisBitrateMode::CBR(args.bitrate),
        })
        .with_buffer_config(BufferConfig {
            buffered_seconds: args.buffer,
            max_chunk_size: 8192,
        });

    let (input_tx, mut output_rx) = mux.spawn();

    // Spawn the task to send data to Icecast
    let icecast_sender = {
        let icecast_client = icecast_client.clone();
        let running = running.clone();

        tokio::spawn(async move {
            while let Some(chunk) = output_rx.recv().await {
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                if let Err(e) = icecast_client.send_data(&chunk).await {
                    error!("Failed to send data to Icecast: {}", e);
                    break;
                }
            }

            debug!("Icecast sender task finished");
        })
    };

    // Spawn the input reader task
    let input_reader = {
        let input_pipe = args.input_pipe.clone();
        let running = running.clone();

        tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match open_pipe(&input_pipe).await {
                    Ok(mut pipe) => {
                        info!("Successfully opened input pipe: {}", input_pipe);
                        let mut buf = [0u8; 8192];

                        while running.load(Ordering::SeqCst) {
                            match pipe.read(&mut buf).await {
                                Ok(0) => {
                                    debug!("End of pipe data, reopening");
                                    break;
                                },
                                Ok(n) => {
                                    let data = Bytes::copy_from_slice(&buf[..n]);
                                    if input_tx.send(data).await.is_err() {
                                        warn!("Failed to send data to OggMux, channel closed");
                                        break;
                                    }
                                },
                                Err(e) => {
                                    warn!("Error reading from pipe: {}", e);
                                    break;
                                }
                            }
                        }
                    },
                    Err(e) => {
                        debug!("Waiting for pipe: {}", e);
                        sleep(Duration::from_secs(1)).await;
                    }
                }

                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }

            debug!("Input reader task finished");
        })
    };

    // Wait for termination signal
    while running.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(100)).await;
    }

    // Clean up
    info!("Shutting down...");

    // Give tasks a moment to finish
    tokio::time::timeout(Duration::from_secs(2), input_reader).await.ok();
    tokio::time::timeout(Duration::from_secs(2), icecast_sender).await.ok();

    // Disconnect from Icecast
    if let Err(e) = icecast_client.disconnect().await {
        error!("Error disconnecting from Icecast: {}", e);
    }

    info!("Shutdown complete");
    Ok(())
}

/// Parse a host string in the format "host" or "host:port"
fn parse_host_port(host_str: &str) -> Result<(String, u16)> {
    validation::parse_host_port(host_str)
}

/// Set up logging with the specified level and format
fn setup_logging(log_level: &str, format: &str) {
    use tracing_subscriber::{fmt, EnvFilter, prelude::*};

    let level = match log_level.to_lowercase().as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "info" => "info",
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    // Set up the subscriber based on format
    match format.to_lowercase().as_str() {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_target(true).with_thread_ids(true))
                .init();
        }
    }
}

/// Set up signal handlers for graceful shutdown
fn setup_signal_handlers(running: Arc<AtomicBool>) -> Result<()> {
    util::setup_signal_handlers(running)
        .map_err(|e| crate::errors::SnowbootError::Internal {
            message: format!("Failed to set up signal handlers: {}", e),
            code: crate::errors::ErrorCode::Unknown,
        })
}

/// Open the input pipe file
async fn open_pipe(path: &str) -> Result<File> {
    // Validate that the path is a FIFO
    validation::validate_fifo(path)?;

    // Open the FIFO
    File::open(path).await
        .map_err(|e| crate::errors::SnowbootError::pipe_open_failed(path, e))
}
