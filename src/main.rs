mod api;
mod config;
mod connection;
mod errors;
mod icecast;
mod metrics;
mod player;
mod queue;
mod server;
mod util;
mod validation;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::errors::Result;
use clap::Parser;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use oggmux::{OggMux, VorbisConfig, VorbisBitrateMode, BufferConfig};

use crate::api::AppState;
use crate::connection::ConnectionState;
use crate::icecast::{IcecastClient, IcecastConfig};
use crate::player::PlayerHandle;
use crate::queue::{Queue, SharedQueue};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "A tool to help with remuxing and streaming Ogg content over Icecast")]
struct Args {
    /// Icecast server hostname (with optional port)
    #[arg(long, value_name = "HOST[:PORT]", default_value = "localhost:8000")]
    host: String,

    /// Mount point on the Icecast server
    #[arg(long, value_name = "PATH", default_value = "/stream.ogg")]
    mount: String,

    /// Username for Icecast authentication
    #[arg(long, value_name = "USERNAME", default_value = "source")]
    user: String,

    /// Password for Icecast authentication
    #[arg(long, value_name = "PASSWORD", default_value = "hackme")]
    password: String,

    /// Sample rate for the Ogg Vorbis stream
    #[arg(long, value_name = "RATE", default_value = "44100")]
    sample_rate: u32,

    /// Bitrate for the Ogg Vorbis stream
    #[arg(long, value_name = "BITRATE", default_value = "320")]
    bitrate: u32,

    /// Buffer size in seconds
    #[arg(long, value_name = "SECONDS", default_value = "1.0")]
    buffer: f64,

    /// API server port
    #[arg(long, value_name = "PORT", default_value = "3000")]
    api_port: u16,

    /// API server bind address
    #[arg(long, value_name = "ADDR", default_value = "0.0.0.0")]
    api_bind: String,

    /// Log level
    #[arg(long, value_name = "LEVEL", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    setup_logging(&args.log_level, "text");

    let (host, port) = validation::parse_host_port(&args.host)?;

    info!("Starting snowboot v{}", env!("CARGO_PKG_VERSION"));
    info!("Connecting to {}:{}{}", host, port, args.mount);

    // Initialise metrics
    metrics::init_metrics();

    let shutdown = CancellationToken::new();

    // Connection state shared with API
    let connection_state = Arc::new(std::sync::Mutex::new(ConnectionState::Disconnected));

    // Configure and connect to Icecast
    let icecast_config = IcecastConfig {
        host,
        port,
        mount: args.mount,
        username: args.user,
        password: args.password,
        content_type: "application/ogg".to_string(),
    };

    let icecast_client = IcecastClient::new(icecast_config);
    icecast_client.connect().await?;

    {
        *connection_state.lock().unwrap() = ConnectionState::Connected;
    }

    // Create queue and player (before mux so we can wire up metadata callback)
    let queue: SharedQueue = Arc::new(tokio::sync::RwLock::new(Queue::default()));
    let player_handle = PlayerHandle::new(queue.clone());

    // Configure and spawn OggMux with metadata callback
    let metadata_player = player_handle.clone();
    let mux = OggMux::new()
        .with_vorbis_config(VorbisConfig {
            sample_rate: args.sample_rate,
            bitrate: VorbisBitrateMode::CBR(args.bitrate),
        })
        .with_buffer_config(BufferConfig {
            buffered_seconds: args.buffer,
            channel_capacity: 8192,
        })
        .with_metadata_callback(move |_granule_pos| {
            metadata_player.now_playing()
                .map(|track| track.metadata_comments())
        });

    let (input_tx, mut output_rx, _shutdown_tx, _mux_handle) = mux.spawn();

    // Spawn the Icecast sender task
    let icecast_sender = {
        let icecast_client = icecast_client.clone();
        let shutdown = shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    chunk = output_rx.recv() => {
                        match chunk {
                            Some(data) => {
                                if let Err(e) = icecast_client.send_data(&data).await {
                                    error!("Failed to send data to Icecast: {}", e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
            debug!("Icecast sender task finished");
        })
    };

    // Spawn player task
    let player_task = {
        let handle = player_handle.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            player::run_player(handle, input_tx, shutdown).await;
        })
    };

    // Build and start the API server
    let start_time = Instant::now();
    let app_state = AppState {
        queue: queue.clone(),
        player: player_handle.clone(),
        start_time,
        connection_state: connection_state.clone(),
    };

    let app = api::router(app_state);
    let addr: SocketAddr = format!("{}:{}", args.api_bind, args.api_port)
        .parse()
        .expect("Invalid API bind address");

    info!("Starting API server on {}", addr);
    let listener = TcpListener::bind(addr).await
        .map_err(|e| errors::SnowbootError::Internal {
            message: format!("Failed to bind API server: {}", e),
            code: errors::ErrorCode::Unknown,
        })?;

    let api_server = {
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await
                .ok();
            debug!("API server stopped");
        })
    };

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await.ok();
    info!("Shutting down...");
    shutdown.cancel();

    // Give tasks time to finish
    tokio::time::timeout(Duration::from_secs(5), async {
        let _ = player_task.await;
        let _ = icecast_sender.await;
        let _ = api_server.await;
    }).await.ok();

    // Disconnect from Icecast
    if let Err(e) = icecast_client.disconnect().await {
        error!("Error disconnecting from Icecast: {}", e);
    }

    info!("Shutdown complete");
    Ok(())
}

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
