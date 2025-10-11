// Custom error types for Snowboot with error codes for programmatic handling

use std::io;
use thiserror::Error;

/// Error codes for programmatic error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Configuration errors (1000-1999)
    InvalidConfig = 1000,
    InvalidPort = 1001,
    InvalidHost = 1002,
    InvalidBufferSize = 1003,
    InvalidSampleRate = 1004,
    InvalidBitrate = 1005,
    ConfigFileNotFound = 1006,
    ConfigParseFailed = 1007,

    /// Connection errors (2000-2999)
    ConnectionFailed = 2000,
    ConnectionTimeout = 2001,
    AuthenticationFailed = 2002,
    UnexpectedResponse = 2003,
    DisconnectedUnexpectedly = 2004,
    TlsError = 2005,

    /// I/O errors (3000-3999)
    PipeNotFound = 3000,
    PipeOpenFailed = 3001,
    PipeReadFailed = 3002,
    NotAFifo = 3003,
    PermissionDenied = 3004,

    /// Protocol errors (4000-4999)
    HttpParseFailed = 4000,
    InvalidHttpResponse = 4001,
    UnsupportedProtocol = 4002,

    /// Internal errors (5000-5999)
    ChannelClosed = 5000,
    TaskPanic = 5001,
    ShutdownFailed = 5002,

    /// Generic error
    Unknown = 9999,
}

impl ErrorCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Main error type for Snowboot operations
#[derive(Error, Debug)]
pub enum SnowbootError {
    #[error("Invalid configuration: {message} (code: {code})")]
    Config {
        message: String,
        code: ErrorCode,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Connection error: {message} (code: {code})")]
    Connection {
        message: String,
        code: ErrorCode,
        #[source]
        source: Option<io::Error>,
    },

    #[error("I/O error: {message} (code: {code})")]
    Io {
        message: String,
        code: ErrorCode,
        #[source]
        source: io::Error,
    },

    #[error("Protocol error: {message} (code: {code})")]
    Protocol {
        message: String,
        code: ErrorCode,
        details: Option<String>,
    },

    #[error("Internal error: {message} (code: {code})")]
    Internal {
        message: String,
        code: ErrorCode,
    },
}

impl SnowbootError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            SnowbootError::Config { code, .. } => *code,
            SnowbootError::Connection { code, .. } => *code,
            SnowbootError::Io { code, .. } => *code,
            SnowbootError::Protocol { code, .. } => *code,
            SnowbootError::Internal { code, .. } => *code,
        }
    }

    pub fn suggestion(&self) -> Option<&str> {
        match self {
            SnowbootError::Config { code: ErrorCode::InvalidPort, .. } => {
                Some("Port must be between 1 and 65535")
            }
            SnowbootError::Config { code: ErrorCode::InvalidBufferSize, .. } => {
                Some("Buffer size should be between 0.1 and 10.0 seconds")
            }
            SnowbootError::Config { code: ErrorCode::InvalidSampleRate, .. } => {
                Some("Sample rate should be between 8000 and 192000 Hz (common: 44100, 48000)")
            }
            SnowbootError::Config { code: ErrorCode::InvalidBitrate, .. } => {
                Some("Bitrate should be between 8 and 500 kbps")
            }
            SnowbootError::Connection { code: ErrorCode::AuthenticationFailed, .. } => {
                Some("Check your username and password. Set via --user/--password or SNOWBOOT_USER/SNOWBOOT_PASSWORD env vars")
            }
            SnowbootError::Connection { code: ErrorCode::ConnectionFailed, .. } => {
                Some("Ensure the Icecast server is running and reachable. Check firewall settings.")
            }
            SnowbootError::Connection { code: ErrorCode::UnexpectedResponse, .. } => {
                Some("The server may not be an Icecast server, or it rejected the connection. Check mount point and credentials.")
            }
            SnowbootError::Io { code: ErrorCode::PipeNotFound, .. } => {
                Some("Create the named pipe with: mkfifo /path/to/pipe")
            }
            SnowbootError::Io { code: ErrorCode::NotAFifo, .. } => {
                Some("The path exists but is not a FIFO. Remove it and create a named pipe with: mkfifo /path/to/pipe")
            }
            SnowbootError::Io { code: ErrorCode::PermissionDenied, .. } => {
                Some("Check file permissions or run with appropriate privileges")
            }
            _ => None,
        }
    }
}

// Helper functions for creating errors
impl SnowbootError {
    pub fn invalid_port(port_str: &str) -> Self {
        SnowbootError::Config {
            message: format!("Invalid port number: {}", port_str),
            code: ErrorCode::InvalidPort,
            source: None,
        }
    }

    pub fn invalid_host(host_str: &str) -> Self {
        SnowbootError::Config {
            message: format!("Invalid host format: {}", host_str),
            code: ErrorCode::InvalidHost,
            source: None,
        }
    }

    pub fn invalid_buffer_size(size: f64) -> Self {
        SnowbootError::Config {
            message: format!("Invalid buffer size: {} seconds", size),
            code: ErrorCode::InvalidBufferSize,
            source: None,
        }
    }

    pub fn invalid_sample_rate(rate: u32) -> Self {
        SnowbootError::Config {
            message: format!("Invalid sample rate: {} Hz", rate),
            code: ErrorCode::InvalidSampleRate,
            source: None,
        }
    }

    pub fn invalid_bitrate(bitrate: u32) -> Self {
        SnowbootError::Config {
            message: format!("Invalid bitrate: {} kbps", bitrate),
            code: ErrorCode::InvalidBitrate,
            source: None,
        }
    }

    pub fn connection_failed(host: &str, port: u16, source: io::Error) -> Self {
        SnowbootError::Connection {
            message: format!("Failed to connect to {}:{}", host, port),
            code: ErrorCode::ConnectionFailed,
            source: Some(source),
        }
    }

    pub fn auth_failed(response: &str) -> Self {
        SnowbootError::Connection {
            message: format!("Authentication failed: {}", response),
            code: ErrorCode::AuthenticationFailed,
            source: None,
        }
    }

    pub fn unexpected_response(response: &str) -> Self {
        SnowbootError::Connection {
            message: format!("Unexpected server response: {}", response),
            code: ErrorCode::UnexpectedResponse,
            source: None,
        }
    }

    pub fn pipe_not_found(path: &str) -> Self {
        SnowbootError::Io {
            message: format!("Input pipe not found: {}", path),
            code: ErrorCode::PipeNotFound,
            source: io::Error::new(io::ErrorKind::NotFound, "pipe not found"),
        }
    }

    pub fn not_a_fifo(path: &str) -> Self {
        SnowbootError::Io {
            message: format!("Path is not a FIFO: {}", path),
            code: ErrorCode::NotAFifo,
            source: io::Error::new(io::ErrorKind::InvalidInput, "not a fifo"),
        }
    }

    pub fn pipe_open_failed(path: &str, source: io::Error) -> Self {
        SnowbootError::Io {
            message: format!("Failed to open pipe: {}", path),
            code: ErrorCode::PipeOpenFailed,
            source,
        }
    }

    pub fn http_parse_failed(details: String) -> Self {
        SnowbootError::Protocol {
            message: "Failed to parse HTTP response".to_string(),
            code: ErrorCode::HttpParseFailed,
            details: Some(details),
        }
    }

    pub fn channel_closed(channel_name: &str) -> Self {
        SnowbootError::Internal {
            message: format!("Channel closed unexpectedly: {}", channel_name),
            code: ErrorCode::ChannelClosed,
        }
    }
}

/// Result type alias for Snowboot operations
pub type Result<T> = std::result::Result<T, SnowbootError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = SnowbootError::invalid_port("99999");
        assert_eq!(err.error_code(), ErrorCode::InvalidPort);
        assert_eq!(err.error_code().as_u32(), 1001);
    }

    #[test]
    fn test_suggestions() {
        let err = SnowbootError::invalid_port("99999");
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("65535"));
    }

    #[test]
    fn test_error_display() {
        let err = SnowbootError::invalid_port("99999");
        let display = format!("{}", err);
        assert!(display.contains("Invalid port number"));
        assert!(display.contains("99999"));
        assert!(display.contains("1001"));
    }
}
