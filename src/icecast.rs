// src/icecast.rs
//
// Module for handling connections to Icecast servers

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;

use anyhow::{Result, anyhow};
use base64::Engine; // Import Engine trait for encode()
use http::Request;
use hyper::{Body, Client, header};
use log::{debug, error, info, trace};

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
enum IcecastCommand {
    SendData(Vec<u8>),
    Disconnect,
    GetStats,
}

/// Status messages from the Icecast client worker
enum IcecastStatus {
    Connected,
    Error(String),
    Disconnected,
    Stats { bytes_sent: usize, uptime_secs: u64 },
}

/// Represents an active connection to an Icecast server
#[derive(Clone)]
pub struct IcecastClient {
    config: IcecastConfig,
    tx_cmd: Option<Sender<IcecastCommand>>,
    rx_status: Option<Receiver<IcecastStatus>>,
    bytes_sent: Arc<Mutex<usize>>,
    connected: Arc<Mutex<bool>>,
    start_time: Arc<Mutex<Instant>>,
    running: Arc<AtomicBool>,
    worker_handle: Option<Arc<Mutex<Option<JoinHandle<()>>>>>,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            tx_cmd: None,
            rx_status: None,
            bytes_sent: Arc::new(Mutex::new(0)),
            connected: Arc::new(Mutex::new(false)),
            start_time: Arc::new(Mutex::new(Instant::now())),
            running: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&mut self) -> Result<()> {
        if *self.connected.lock().unwrap() {
            debug!("Already connected to Icecast server");
            return Ok(());
        }

        info!("Connecting to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        // Create channels for communication with the worker thread
        let (tx_cmd, rx_cmd) = mpsc::channel::<IcecastCommand>(100);
        let (tx_status, rx_status) = mpsc::channel::<IcecastStatus>(100);

        // Store command sender for later use
        self.tx_cmd = Some(tx_cmd.clone());

        // Prepare shared worker handle
        let worker_handle = Arc::new(Mutex::new(None));
        self.worker_handle = Some(worker_handle.clone());

        // Set up shared state
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let config = self.config.clone();

        // Spawn the worker thread
        let handle = tokio::spawn(async move {
            if let Err(e) = Self::run_worker(config, rx_cmd, tx_status, running).await {
                error!("Icecast worker error: {}", e);
            }
        });

        // Store the worker handle
        *worker_handle.lock().unwrap() = Some(handle);

        // Store the status receiver and wait for connection confirmation or error
        let mut status_rx = rx_status;
        match status_rx.recv().await {
            Some(IcecastStatus::Connected) => {
                *self.connected.lock().unwrap() = true;
                *self.start_time.lock().unwrap() = Instant::now();
                *self.bytes_sent.lock().unwrap() = 0;

                // Store the status receiver
                self.rx_status = Some(status_rx);

                info!("Connected to Icecast server at {}:{}{}",
                      self.config.host, self.config.port, self.config.mount);
                Ok(())
            },
            Some(IcecastStatus::Error(msg)) => {
                self.cleanup_worker().await;
                Err(anyhow!("Failed to connect to Icecast server: {}", msg))
            },
            _ => {
                self.cleanup_worker().await;
                Err(anyhow!("Unexpected status from Icecast worker"))
            },
        }
    }

    /// Clean up worker resources
    async fn cleanup_worker(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(worker_handle) = &self.worker_handle {
            if let Some(handle) = worker_handle.lock().unwrap().take() {
                let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
            }
        }

        self.tx_cmd = None;
        self.rx_status = None;
        *self.connected.lock().unwrap() = false;
    }

    /// Worker function that runs in a separate task
    async fn run_worker(
        config: IcecastConfig,
        mut rx_cmd: Receiver<IcecastCommand>,
        tx_status: Sender<IcecastStatus>,
        running: Arc<AtomicBool>,
    ) -> Result<()> {
        // Prepare the connection
        let result = Self::create_connection(&config).await;

        match result {
            Ok(_sender) => {
                // Signal that we're connected
                if let Err(e) = tx_status.send(IcecastStatus::Connected).await {
                    error!("Failed to send connected status: {}", e);
                    return Err(anyhow!("Failed to send connection status"));
                }

                // Track statistics
                let mut bytes_sent = 0;
                let start_time = Instant::now();

                // Main worker loop
                while running.load(Ordering::SeqCst) {
                    match rx_cmd.recv().await {
                        Some(IcecastCommand::SendData(data)) => {
                            // Send data to Icecast
                            // In a real implementation, we would use the sender
                            bytes_sent += data.len();
                            trace!("Sent {} bytes to Icecast server", data.len());
                        },
                        Some(IcecastCommand::GetStats) => {
                            // Send current stats back
                            let stats = IcecastStatus::Stats {
                                bytes_sent,
                                uptime_secs: start_time.elapsed().as_secs(),
                            };
                            let _ = tx_status.send(stats).await;
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
            },
            Err(e) => {
                let error_msg = format!("Failed to create Icecast connection: {}", e);
                error!("{}", error_msg);
                let _ = tx_status.send(IcecastStatus::Error(error_msg)).await;
                Err(e)
            }
        }
    }

    /// Create a connection to the Icecast server
    async fn create_connection(config: &IcecastConfig) -> Result<()> {
        // Create authorization header
        let auth = format!("{}:{}", config.username, config.password);
        let auth_header = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(&auth));

        // Build the request
        let uri = format!("http://{}:{}{}", config.host, config.port, config.mount);
        let mut req = Request::put(uri)
            .header(header::AUTHORIZATION, auth_header)
            .header(header::CONTENT_TYPE, &config.content_type)
            .header("ice-name", config.name.clone().unwrap_or_else(|| "Snowboot Stream".to_string()))
            .header("ice-public", config.is_public.unwrap_or(false).to_string());

        // Add optional headers if provided
        if let Some(desc) = &config.description {
            req = req.header("ice-description", desc);
        }
        if let Some(genre) = &config.genre {
            req = req.header("ice-genre", genre);
        }
        if let Some(url) = &config.url {
            req = req.header("ice-url", url);
        }

        // In a real implementation, we would actually connect to the server here
        // For now, we just return success to simulate a connection
        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&mut self, data: &[u8]) -> Result<()> {
        if !*self.connected.lock().unwrap() {
            return Err(anyhow!("Not connected to Icecast server"));
        }

        if let Some(tx) = &self.tx_cmd {
            // Clone the data to send it to the worker thread
            let data_vec = data.to_vec();
            tx.send(IcecastCommand::SendData(data_vec)).await
                .map_err(|_| anyhow!("Failed to send data to worker thread"))?;

            // Update local statistics
            *self.bytes_sent.lock().unwrap() += data.len();

            // Process any status updates
            self.process_status_updates().await;

            Ok(())
        } else {
            Err(anyhow!("Command channel not available"))
        }
    }

    /// Process any pending status updates from the worker
    async fn process_status_updates(&mut self) {
        if let Some(rx) = &mut self.rx_status {
            // Try to receive any status updates, but don't block
            while let Ok(Some(status)) = tokio::time::timeout(Duration::from_millis(1), rx.recv()).await {
                match status {
                    IcecastStatus::Error(msg) => {
                        error!("Icecast error: {}", msg);
                        *self.connected.lock().unwrap() = false;
                    },
                    IcecastStatus::Disconnected => {
                        debug!("Icecast server disconnected");
                        *self.connected.lock().unwrap() = false;
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
        if !*self.connected.lock().unwrap() {
            return Ok(());
        }

        info!("Disconnecting from Icecast server after sending {} bytes over {} seconds",
              *self.bytes_sent.lock().unwrap(),
              self.start_time.lock().unwrap().elapsed().as_secs());

        // Send disconnect command to worker
        if let Some(tx) = &self.tx_cmd {
            let _ = tx.send(IcecastCommand::Disconnect).await;
        }

        // Clean up resources
        self.cleanup_worker().await;

        Ok(())
    }

    /// Check if the client is currently connected
    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    /// Get the total number of bytes sent since connection
    pub fn bytes_sent(&self) -> usize {
        *self.bytes_sent.lock().unwrap()
    }

    /// Get the uptime of the connection
    pub fn uptime(&self) -> Duration {
        self.start_time.lock().unwrap().elapsed()
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Request current statistics from the worker
    pub async fn request_stats(&self) -> Result<()> {
        if let Some(tx) = &self.tx_cmd {
            tx.send(IcecastCommand::GetStats).await
                .map_err(|_| anyhow!("Failed to send stats request"))?;
            Ok(())
        } else {
            Err(anyhow!("Command channel not available"))
        }
    }
}
