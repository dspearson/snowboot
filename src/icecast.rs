
// src/icecast.rs
//
// Module for handling connections to Icecast servers

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;

use anyhow::{Result, anyhow};
use http::Request;
use hyper::{Body, Client, header};
use log::{debug, info, trace, warn, error};

/// Configuration for the Icecast connection
#[derive(Clone)]
pub struct IcecastConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    pub password: String,
    pub content_type: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub url: Option<String>,
    pub is_public: Option<bool>,
}

impl Default for IcecastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream.ogg".to_string(),
            username: "source".to_string(), // Default username for Icecast
            password: "hackme".to_string(), // Default password, should be changed!
            content_type: "application/ogg".to_string(),
            name: None,
            description: None,
            genre: None,
            url: None,
            is_public: None,
        }
    }
}

/// Commands for the Icecast client worker thread
pub enum IcecastCommand {
    SendData(Vec<u8>),
    Disconnect,
}

/// Status messages from the Icecast client worker
pub enum IcecastStatus {
    Connected,
    Error(String),
    Disconnected,
    Stats { bytes_sent: usize, uptime_secs: u64 },
}

/// Represents an active connection to an Icecast server
pub struct IcecastClient {
    config: IcecastConfig,
    tx_cmd: Option<Sender<IcecastCommand>>,
    rx_status: Option<Receiver<IcecastStatus>>,
    bytes_sent: usize,
    connected: bool,
    start_time: Instant,
    running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            tx_cmd: None,
            rx_status: None,
            bytes_sent: 0,
            connected: false,
            start_time: Instant::now(),
            running: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&mut self) -> Result<()> {
        if self.connected {
            debug!("Already connected to Icecast server");
            return Ok(());
        }

        info!("Connecting to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        // Create channels for communication with the worker thread
        let (tx_cmd, rx_cmd) = mpsc::channel::<IcecastCommand>(100);
        let (tx_status, rx_status) = mpsc::channel::<IcecastStatus>(100);

        // Store channels for later use
        self.tx_cmd = Some(tx_cmd);
        self.rx_status = Some(rx_status);

        // Set up shared state
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let config = self.config.clone();

        // Spawn the worker thread
        self.worker_handle = Some(tokio::spawn(async move {
            if let Err(e) = Self::run_worker(config, rx_cmd, tx_status.clone(), running).await {
                let error_msg = format!("Icecast worker error: {}", e);
                error!("{}", error_msg);
                // Send error back to main thread
                if let Err(send_err) = tx_status.send(IcecastStatus::Error(error_msg)).await {
                    error!("Failed to send error status: {}", send_err);
                }
            }
        }));

        // Wait for connection confirmation or error
        if let Some(mut rx) = &mut self.rx_status {
            match rx.recv().await {
                Some(IcecastStatus::Connected) => {
                    self.connected = true;
                    self.start_time = Instant::now();
                    info!("Connected to Icecast server at {}:{}{}",
                          self.config.host, self.config.port, self.config.mount);
                    Ok(())
                },
                Some(IcecastStatus::Error(msg)) => {
                    Err(anyhow!("Failed to connect to Icecast server: {}", msg))
                },
                _ => Err(anyhow!("Unexpected status from Icecast worker")),
            }
        } else {
            Err(anyhow!("Status channel not available"))
        }
    }

    /// Worker function that runs in a separate task
    async fn run_worker(
        config: IcecastConfig,
        mut rx_cmd: Receiver<IcecastCommand>,
        tx_status: Sender<IcecastStatus>,
        running: Arc<AtomicBool>,
    ) -> Result<()> {
        // Create the connection
        let connection = Self::create_connection(&config).await?;

        // Signal that we're connected
        tx_status.send(IcecastStatus::Connected).await
            .map_err(|e| anyhow!("Failed to send connected status: {}", e))?;

        // Track statistics
        let mut bytes_sent = 0;
        let start_time = Instant::now();

        // Main worker loop
        while running.load(Ordering::SeqCst) {
            match rx_cmd.recv().await {
                Some(IcecastCommand::SendData(data)) => {
                    // Here you would send the data to the Icecast server
                    // For now, we just track statistics
                    bytes_sent += data.len();
                    trace!("Sent {} bytes to Icecast server", data.len());

                    // Periodically send stats back to main thread (every ~5 seconds)
                    if bytes_sent % 1_000_000 < 1024 {
                        let _ = tx_status.send(IcecastStatus::Stats {
                            bytes_sent,
                            uptime_secs: start_time.elapsed().as_secs(),
                        }).await;
                    }
                },
                Some(IcecastCommand::Disconnect) => {
                    info!("Received disconnect command after sending {} bytes over {} seconds",
                          bytes_sent, start_time.elapsed().as_secs());
                    break;
                },
                None => {
                    debug!("Command channel closed, disconnecting");
                    break;
                }
            }
        }

        // Send disconnected status
        let _ = tx_status.send(IcecastStatus::Disconnected).await;

        Ok(())
    }

    /// Create a connection to the Icecast server
    async fn create_connection(_config: &IcecastConfig) -> Result<()> {
        // This would be where you set up the actual connection
        // For this example, we're just simulating it

        // Create authorization header
        let auth = format!("{}:{}", _config.username, _config.password);
        let auth_header = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(&auth));

        // Build the request
        let uri = format!("http://{}:{}{}", _config.host, _config.port, _config.mount);
        let mut req = Request::put(uri)
            .header(header::AUTHORIZATION, auth_header)
            .header(header::CONTENT_TYPE, &_config.content_type)
            .header("ice-name", _config.name.clone().unwrap_or_else(|| "Snowboot Stream".to_string()))
            .header("ice-public", _config.is_public.unwrap_or(false).to_string());

        // Add optional headers if provided
        if let Some(desc) = &_config.description {
            req = req.header("ice-description", desc);
        }
        if let Some(genre) = &_config.genre {
            req = req.header("ice-genre", genre);
        }
        if let Some(url) = &_config.url {
            req = req.header("ice-url", url);
        }

        // In a real implementation, you would create and store the body sender
        // to write data to later, but for simplicity, we'll just simulate it
        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&mut self, data: &[u8]) -> Result<()> {
        if !self.connected {
            return Err(anyhow!("Not connected to Icecast server"));
        }

        if let Some(tx) = &self.tx_cmd {
            // Clone the data to send it to the worker thread
            let data_vec = data.to_vec();
            tx.send(IcecastCommand::SendData(data_vec)).await
                .map_err(|_| anyhow!("Failed to send data to worker thread"))?;

            // Update local statistics
            self.bytes_sent += data.len();

            // Process any status updates
            self.process_status_updates().await;

            Ok(())
        } else {
            Err(anyhow!("Command channel not available"))
        }
    }

    /// Process any pending status updates
    async fn process_status_updates(&mut self) {
        if let Some(rx) = &mut self.rx_status {
            // Try to receive any status updates, but don't block
            while let Ok(Some(status)) = tokio::time::timeout(Duration::from_millis(1), rx.recv()).await {
                match status {
                    IcecastStatus::Error(msg) => {
                        error!("Icecast error: {}", msg);
                        self.connected = false;
                    },
                    IcecastStatus::Disconnected => {
                        debug!("Icecast server disconnected");
                        self.connected = false;
                    },
                    IcecastStatus::Stats { bytes_sent, uptime_secs } => {
                        trace!("Icecast stats: {} bytes sent over {} seconds", bytes_sent, uptime_secs);
                    },
                    _ => { /* Ignore other status updates */ }
                }
            }
        }
    }

    /// Disconnect from the Icecast server
    pub async fn disconnect(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }

        info!("Disconnecting from Icecast server after sending {} bytes over {} seconds",
              self.bytes_sent,
              self.start_time.elapsed().as_secs());

        // Send disconnect command to worker
        if let Some(tx) = &self.tx_cmd {
            let _ = tx.send(IcecastCommand::Disconnect).await;
        }

        // Stop the worker
        self.running.store(false, Ordering::SeqCst);

        // Wait for the worker to exit
        if let Some(handle) = self.worker_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }

        self.connected = false;
        self.tx_cmd = None;
        self.rx_status = None;

        Ok(())
    }

    /// Check if the client is currently connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get the total number of bytes sent since connection
    pub fn bytes_sent(&self) -> usize {
        self.bytes_sent
    }

    /// Get the uptime of the connection
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
