// Module for handling connections to Icecast servers

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::errors::{Result, SnowbootError, ErrorCode};
use tracing::{error, info, trace, debug, instrument};
use httparse;

/// Configuration for the Icecast connection
#[derive(Clone)]
pub struct IcecastConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    pub password: String,
    pub content_type: String,
}

impl Default for IcecastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream.ogg".to_string(),
            username: "source".to_string(),
            password: "hackme".to_string(),
            content_type: "application/ogg".to_string(),
        }
    }
}

/// Represents an active connection to an Icecast server
#[derive(Clone)]
pub struct IcecastClient {
    config: IcecastConfig,
    running: Arc<AtomicBool>,
    stream: Arc<Mutex<Option<TcpStream>>>,
}

impl IcecastClient {
    /// Create a new Icecast client with the given configuration
    pub fn new(config: IcecastConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            stream: Arc::new(Mutex::new(None)),
        }
    }

    /// Connect to the Icecast server
    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to Icecast server at {}:{}{}",
              self.config.host, self.config.port, self.config.mount);

        // Connect to the server
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let mut stream = TcpStream::connect(&addr).await
            .map_err(|e| SnowbootError::connection_failed(&self.config.host, self.config.port, e))?;

        // Set TCP_NODELAY to reduce latency
        stream.set_nodelay(true).map_err(|e| SnowbootError::Connection {
            message: "Failed to set TCP_NODELAY".to_string(),
            code: ErrorCode::ConnectionFailed,
            source: Some(e),
        })?;

        // Create HTTP PUT request with authentication
        let auth = format!("{}:{}", self.config.username, self.config.password);
        let auth_header = format!("Basic {}", BASE64.encode(auth));

        // Build the PUT request
        let request = format!(
            "PUT {} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Authorization: {}\r\n\
             Content-Type: {}\r\n\
             Ice-Public: 1\r\n\
             Ice-Name: Snowboot Stream\r\n\
             Ice-Description: Powered by Snowboot\r\n\
             User-Agent: Snowboot/0.1.0\r\n\
             Expect: 100-continue\r\n\
             \r\n",
            self.config.mount,
            self.config.host,
            self.config.port,
            auth_header,
            self.config.content_type
        );

        // Send the request
        stream.write_all(request.as_bytes()).await
            .map_err(|e| SnowbootError::Connection {
                message: "Failed to send HTTP request".to_string(),
                code: ErrorCode::ConnectionFailed,
                source: Some(e),
            })?;

        // Receive the server's response - read until we get complete headers
        let response_str = self.read_http_response(&mut stream).await?;

        // Parse the HTTP response properly
        let status_code = self.parse_http_status(&response_str)?;

        debug!("Received HTTP status code: {}", status_code);

        // Check if the response is HTTP 100 Continue or 200 OK
        if status_code != 100 && status_code != 200 {
            if status_code == 401 || status_code == 403 {
                return Err(SnowbootError::auth_failed(&response_str));
            }
            return Err(SnowbootError::unexpected_response(&response_str));
        }

        // Save the stream
        *self.stream.lock().await = Some(stream);

        // Set the client as running
        self.running.store(true, Ordering::SeqCst);
        info!("Successfully connected to Icecast server");

        Ok(())
    }

    /// Send data to the Icecast server
    pub async fn send_data(&self, data: &[u8]) -> Result<()> {
        if !self.is_running() {
            return Err(SnowbootError::Connection {
                message: "Not connected to Icecast server".to_string(),
                code: ErrorCode::DisconnectedUnexpectedly,
                source: None,
            });
        }

        let mut stream_guard = self.stream.lock().await;
        if let Some(stream) = &mut *stream_guard {
            match stream.write_all(data).await {
                Ok(_) => {
                    trace!("Sent {} bytes to Icecast", data.len());
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to send data to Icecast: {}", e);
                    self.running.store(false, Ordering::SeqCst);
                    *stream_guard = None;
                    Err(SnowbootError::Connection {
                        message: "Failed to send data to server".to_string(),
                        code: ErrorCode::DisconnectedUnexpectedly,
                        source: Some(e),
                    })
                }
            }
        } else {
            Err(SnowbootError::Connection {
                message: "Stream disconnected".to_string(),
                code: ErrorCode::DisconnectedUnexpectedly,
                source: None,
            })
        }
    }

    /// Disconnect from the Icecast server
    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from Icecast server");

        let mut stream_guard = self.stream.lock().await;
        if let Some(mut stream) = stream_guard.take() {
            // Properly close the connection
            let _ = stream.shutdown().await;
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Read HTTP response from stream until we get complete headers
    async fn read_http_response(&self, stream: &mut TcpStream) -> Result<String> {
        let mut buffer = Vec::with_capacity(4096);
        let mut temp = [0u8; 1024];

        // Read until we find \r\n\r\n (end of headers)
        loop {
            let n = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stream.read(&mut temp)
            ).await
                .map_err(|_| SnowbootError::Connection {
                    message: "Timeout reading server response".to_string(),
                    code: ErrorCode::ConnectionTimeout,
                    source: None,
                })?
                .map_err(|e| SnowbootError::Connection {
                    message: "Failed to read server response".to_string(),
                    code: ErrorCode::ConnectionFailed,
                    source: Some(e),
                })?;

            if n == 0 {
                return Err(SnowbootError::Connection {
                    message: "Server closed connection before sending response".to_string(),
                    code: ErrorCode::DisconnectedUnexpectedly,
                    source: None,
                });
            }

            buffer.extend_from_slice(&temp[..n]);

            // Check if we have complete headers
            if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }

            // Prevent infinite buffering
            if buffer.len() > 16384 {
                return Err(SnowbootError::http_parse_failed(
                    "Response headers too large".to_string()
                ));
            }
        }

        String::from_utf8(buffer)
            .map_err(|e| SnowbootError::http_parse_failed(format!("Invalid UTF-8: {}", e)))
    }

    /// Parse HTTP status code from response
    fn parse_http_status(&self, response: &str) -> Result<u16> {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut resp = httparse::Response::new(&mut headers);

        match resp.parse(response.as_bytes()) {
            Ok(httparse::Status::Complete(_)) => {
                resp.code.ok_or_else(|| {
                    SnowbootError::http_parse_failed("No status code in response".to_string())
                })
            }
            Ok(httparse::Status::Partial) => {
                Err(SnowbootError::http_parse_failed("Incomplete HTTP response".to_string()))
            }
            Err(e) => {
                Err(SnowbootError::http_parse_failed(format!("Parse error: {}", e)))
            }
        }
    }
}
