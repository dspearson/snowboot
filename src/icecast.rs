// src/icecast.r// src/icecast.rs
//
// Module for handling connections to Icecast servers

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinHandle;

use anyhow::{Result, anyhow};
use log::{debug, error, info};

/// Configuration for the Icecast connection
#[derive(Clone)]
pub struct IcecastConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    pub password: String,
}

impl Default for IcecastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream.ogg".to_string(),
            username: "source".to_string(),
            password: "hackme".to_string(),
        }
    }
}

/// Commands for the Icecast client worker thread
enum IcecastCommand {
    SendData(Vec<u8>),
    Disconnect,
}

/// Represents an active connection to an Icecast server
#[derive(Clone)]
pub struct IcecastClient {
    config: IcecastConfig,
    running: Arc<AtomicBool>,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&mut self, data: &[u8]) -> Result<()> {
        // Placeholder implementation
        let _ = data;
        Ok(())
    }

    /// Disconnect from the Icecast server
    pub async fn disconnect(&mut self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if the client is currently connected
    pub fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
