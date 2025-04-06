// src/icecast.rs
//
// Module for handling connections to Icecast servers

use std::io;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use base64::{engine::general_purpose, Engine as _};
use log::{error, info, trace, warn};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

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

/// Represents an active connection to an Icecast server
pub struct IcecastClient {
    config: IcecastConfig,
    writer: Arc<Mutex<Option<BufWriter<TcpStream>>>>,
    connected: bool,
    connect_timeout: Duration,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            writer: Arc::new(Mutex::new(None)),
            connected: false,
            connect_timeout: Duration::from_secs(5),
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&mut self) -> Result<()> {
        if self.connected {
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.host, self.config.port);
        info!("Connecting to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        // Establish TCP connection with timeout
        let stream = tokio::time::timeout(
            self.connect_timeout,
            TcpStream::connect(&addr)
        ).await
            .map_err(|_| anyhow!("Connection timeout"))??;

        // Prepare HTTP headers for PUT request
        let auth = general_purpose::STANDARD.encode(
            format!("{}:{}", self.config.username, self.config.password)
        );

        let request = format!(
            "PUT {} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Authorization: Basic {}\r\n\
             Content-Type: application/ogg\r\n\
             Expect: 100-continue\r\n\
             \r\n",
            self.config.mount,
            self.config.host,
            self.config.port,
            auth
        );

        let mut writer = BufWriter::new(stream);
        writer.write_all(request.as_bytes()).await?;
        writer.flush().await?;

        // Read the initial HTTP response
        let mut response_buf = [0u8; 1024];
        let socket_clone = writer.get_ref().try_clone()?;

        // Set a read timeout for the initial response
        socket_clone.set_read_timeout(Some(self.connect_timeout))?;

        let n = socket_clone.read(&mut response_buf)?;
        let response = String::from_utf8_lossy(&response_buf[0..n]);

        // Check for a successful response
        if !response.contains("HTTP/1.1 200 OK") && !response.contains("HTTP/1.0 200 OK") {
            return Err(anyhow!("Icecast server returned error: {}", response));
        }

        // Store the writer for future use
        *self.writer.lock().await = Some(writer);
        self.connected = true;

        info!("Successfully connected to Icecast server");
        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&mut self, data: &[u8]) -> Result<usize> {
        if !self.connected {
            self.connect().await?;
        }

        let mut writer_guard = self.writer.lock().await;
        let writer = writer_guard.as_mut()
            .ok_or_else(|| anyhow!("Not connected to Icecast server"))?;

        match writer.write(data).await {
            Ok(bytes_written) => {
                trace!("Sent {} bytes to Icecast", bytes_written);

                // Periodically flush to ensure data is sent
                if bytes_written > 8192 {
                    if let Err(e) = writer.flush().await {
                        error!("Error flushing Icecast writer: {}", e);
                        self.connected = false;
                        *writer_guard = None;
                        return Err(anyhow!("Failed to flush data to Icecast: {}", e));
                    }
                }

                Ok(bytes_written)
            },
            Err(e) => {
                error!("Error writing to Icecast: {}", e);
                self.connected = false;
                *writer_guard = None;
                Err(anyhow!("Failed to write to Icecast stream: {}", e))
            }
        }
    }

    /// Flush any buffered data to the server
    pub async fn flush(&mut self) -> Result<()> {
        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            writer.flush().await?;
        }
        Ok(())
    }

    /// Disconnect from the Icecast server
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.connected {
            info!("Disconnecting from Icecast server");

            // Flush any remaining data
            if let Err(e) = self.flush().await {
                warn!("Error flushing data during disconnect: {}", e);
            }

            // Clear the writer
            *self.writer.lock().await = None;
            self.connected = false;
        }
        Ok(())
    }

    /// Check if the client is currently connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

// Add TryClone trait for TcpStream
trait TryClone {
    fn try_clone(&self) -> io::Result<Self> where Self: Sized;
}

impl TryClone for TcpStream {
    fn try_clone(&self) -> io::Result<Self> {
        // Call the actual try_clone method
        TcpStream::try_clone(self)
    }
}

// Read extension for TcpStream
trait ReadExt {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
}

impl ReadExt for TcpStream {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        // This is a synchronous read from an async TcpStream which is not ideal,
        // but we only need it for the initial HTTP response
        let std_stream = self.try_clone()?.into_std()?;
        std::io::Read::read(&mut &std_stream, buf)
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        let std_stream = self.try_clone()?.into_std()?;
        std_stream.set_read_timeout(timeout)
    }
}
