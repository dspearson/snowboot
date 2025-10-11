// Connection manager with automatic reconnection and exponential backoff

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error, debug, instrument};

use crate::errors::{Result, SnowbootError, ErrorCode};
use crate::icecast::{IcecastClient, IcecastConfig};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

/// Configuration for connection retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = infinite)
    pub max_retries: u32,
    /// Initial backoff duration in seconds
    pub initial_backoff_secs: f64,
    /// Maximum backoff duration in seconds
    pub max_backoff_secs: f64,
    /// Backoff multiplier (exponential)
    pub backoff_multiplier: f64,
    /// Timeout for connection attempts in seconds
    pub connection_timeout_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0, // Infinite retries
            initial_backoff_secs: 1.0,
            max_backoff_secs: 60.0,
            backoff_multiplier: 2.0,
            connection_timeout_secs: 30,
        }
    }
}

/// Connection manager with automatic reconnection
pub struct ConnectionManager {
    client: Arc<IcecastClient>,
    retry_config: RetryConfig,
    state: Arc<std::sync::Mutex<ConnectionState>>,
    retry_count: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl ConnectionManager {
    pub fn new(config: IcecastConfig, retry_config: RetryConfig) -> Self {
        Self {
            client: Arc::new(IcecastClient::new(config)),
            retry_config,
            state: Arc::new(std::sync::Mutex::new(ConnectionState::Disconnected)),
            retry_count: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        *self.state.lock().unwrap()
    }

    /// Get the current retry count
    pub fn retry_count(&self) -> u64 {
        self.retry_count.load(Ordering::Relaxed)
    }

    /// Set the running flag
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::SeqCst);
    }

    /// Connect with automatic retry and exponential backoff
    pub async fn connect(&self) -> Result<()> {
        let mut retry_count = 0u64;
        let mut backoff_secs = self.retry_config.initial_backoff_secs;

        loop {
            if !self.running.load(Ordering::SeqCst) {
                return Err(SnowbootError::Internal {
                    message: "Connection manager stopped".to_string(),
                    code: ErrorCode::ShutdownFailed,
                });
            }

            // Update state
            {
                let mut state = self.state.lock().unwrap();
                *state = if retry_count == 0 {
                    ConnectionState::Connecting
                } else {
                    ConnectionState::Reconnecting
                };
            }

            // Attempt connection
            debug!("Connection attempt {} (backoff: {:.1}s)", retry_count + 1, backoff_secs);

            match tokio::time::timeout(
                Duration::from_secs(self.retry_config.connection_timeout_secs),
                self.client.connect()
            ).await {
                Ok(Ok(())) => {
                    // Success!
                    info!("Successfully connected to Icecast server");
                    *self.state.lock().unwrap() = ConnectionState::Connected;
                    self.retry_count.store(retry_count, Ordering::Relaxed);
                    return Ok(());
                }
                Ok(Err(e)) => {
                    // Connection failed
                    warn!("Connection attempt {} failed: {}", retry_count + 1, e);

                    // Check if it's an auth error (no point retrying)
                    if e.error_code() == ErrorCode::AuthenticationFailed {
                        *self.state.lock().unwrap() = ConnectionState::Failed;
                        return Err(e);
                    }
                }
                Err(_) => {
                    warn!("Connection attempt {} timed out after {}s",
                          retry_count + 1, self.retry_config.connection_timeout_secs);
                }
            }

            // Check retry limit
            retry_count += 1;
            if self.retry_config.max_retries > 0 && retry_count >= self.retry_config.max_retries as u64 {
                error!("Max retry attempts ({}) reached, giving up", self.retry_config.max_retries);
                *self.state.lock().unwrap() = ConnectionState::Failed;
                return Err(SnowbootError::Connection {
                    message: format!("Failed to connect after {} attempts", retry_count),
                    code: ErrorCode::ConnectionFailed,
                    source: None,
                });
            }

            // Calculate next backoff (exponential)
            backoff_secs = (backoff_secs * self.retry_config.backoff_multiplier)
                .min(self.retry_config.max_backoff_secs);

            info!("Retrying connection in {:.1} seconds...", backoff_secs);
            sleep(Duration::from_secs_f64(backoff_secs)).await;
        }
    }

    /// Send data with automatic reconnection on failure
    pub async fn send_data(&self, data: &[u8]) -> Result<()> {
        match self.client.send_data(data).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Connection lost, attempt to reconnect
                warn!("Lost connection while sending data: {}", e);
                *self.state.lock().unwrap() = ConnectionState::Reconnecting;

                // Try to reconnect
                self.connect().await?;

                // Retry sending the data
                self.client.send_data(data).await
            }
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        *self.state.lock().unwrap() = ConnectionState::Disconnected;
        self.client.disconnect().await
    }

    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        self.state() == ConnectionState::Connected
    }

    /// Get a reference to the underlying client
    pub fn client(&self) -> &Arc<IcecastClient> {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 0); // Infinite
        assert_eq!(config.initial_backoff_secs, 1.0);
        assert_eq!(config.max_backoff_secs, 60.0);
    }

    #[test]
    fn test_connection_state() {
        let icecast_config = IcecastConfig::default();
        let retry_config = RetryConfig::default();
        let manager = ConnectionManager::new(icecast_config, retry_config);

        assert_eq!(manager.state(), ConnectionState::Disconnected);
        assert_eq!(manager.retry_count(), 0);
    }

    #[test]
    fn test_backoff_calculation() {
        let mut backoff = 1.0;
        let multiplier = 2.0;
        let max = 60.0;

        backoff = (backoff * multiplier).min(max);
        assert_eq!(backoff, 2.0);

        backoff = (backoff * multiplier).min(max);
        assert_eq!(backoff, 4.0);

        backoff = (backoff * multiplier).min(max);
        assert_eq!(backoff, 8.0);

        // Keep going until we hit max
        for _ in 0..10 {
            backoff = (backoff * multiplier).min(max);
        }
        assert_eq!(backoff, 60.0);
    }
}
