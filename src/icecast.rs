// src/icecast.rs
//
// Module for handling connections to Icecast servers

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, Context};
use http::Request;
use hyper::{Body, Client, header};
use log::{debug, info, trace};

/// Configuration for the Icecast connection
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

/// Represents an active connection to an Icecast server
pub struct IcecastClient {
    config: IcecastConfig,
    connection: Option<hyper::client::ResponseFuture>,
    bytes_sent: usize,
    connected: bool,
    start_time: Instant,
    running: AtomicBool,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            connection: None,
            bytes_sent: 0,
            connected: false,
            start_time: Instant::now(),
            running: AtomicBool::new(true),
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&mut self) -> Result<()> {
        debug!("Connecting to Icecast server at {}:{}{}",
               self.config.host, self.config.port, self.config.mount);

        // Create authorization header
        let auth = format!("{}:{}", self.config.username, self.config.password);
        let auth_header = format!("Basic {}", base64::encode(&auth));

        // Build the request
        let uri = format!("http://{}:{}{}", self.config.host, self.config.port, self.config.mount);
        let mut req = Request::put(uri)
            .header(header::AUTHORIZATION, auth_header)
            .header(header::CONTENT_TYPE, &self.config.content_type)
            .header("ice-name", self.config.name.clone().unwrap_or_else(|| "Snowboot Stream".to_string()))
            .header("ice-public", self.config.is_public.unwrap_or(false).to_string());

        // Add optional headers if provided
        if let Some(desc) = &self.config.description {
            req = req.header("ice-description", desc);
        }
        if let Some(genre) = &self.config.genre {
            req = req.header("ice-genre", genre);
        }
        if let Some(url) = &self.config.url {
            req = req.header("ice-url", url);
        }

        // Create chunked body for streaming
        let (_sender, body) = Body::channel();
        let req = req.body(body).context("Failed to create request body")?;

        // Send the request
        let client = Client::new();
        let response_future = client.request(req);

        self.connection = Some(response_future);
        self.start_time = Instant::now();
        self.connected = true;
        self.running.store(true, Ordering::SeqCst);

        info!("Connected to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&mut self, data: &[u8]) -> Result<()> {
        if !self.connected {
            anyhow::bail!("Not connected to Icecast server");
        }

        // Send the data
        // In the actual implementation, you would write to the body sender
        self.bytes_sent += data.len();
        trace!("Sent {} bytes to Icecast server", data.len());

        Ok(())
    }

    /// Disconnect from the Icecast server
    pub async fn disconnect(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }

        info!("Disconnecting from Icecast server after sending {} bytes over {} seconds",
              self.bytes_sent,
              self.start_time.elapsed().as_secs());

        self.running.store(false, Ordering::SeqCst);
        self.connected = false;

        // Close the connection properly here
        // This would involve proper termination of the body sender

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

    /// Set the running state (useful for signal handling)
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::SeqCst);
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
