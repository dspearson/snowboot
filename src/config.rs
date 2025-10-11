// Configuration management with environment variables, TOML files, and validation

use std::env;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::errors::{Result, SnowbootError};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Icecast server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Audio stream configuration
    #[serde(default)]
    pub audio: AudioConfig,

    /// Input configuration
    #[serde(default)]
    pub input: InputConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,

    /// Monitoring configuration
    #[serde(default)]
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    #[serde(skip_serializing)] // Never serialize passwords
    pub password: String,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub bitrate: u32,
    pub buffer_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub pipe_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_enabled: bool,
    pub metrics_port: u16,
    pub health_enabled: bool,
    pub health_port: u16,
}

// Default implementations
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream.ogg".to_string(),
            username: "source".to_string(),
            password: "hackme".to_string(),
            use_tls: false,
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            bitrate: 320,
            buffer_seconds: 1.0,
        }
    }
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            pipe_path: "/tmp/snowboot.in".to_string(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: LogFormat::Text,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: false,
            metrics_port: 9090,
            health_enabled: false,
            health_port: 8080,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            audio: AudioConfig::default(),
            input: InputConfig::default(),
            logging: LoggingConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| SnowbootError::Config {
                message: format!("Failed to read config file: {}", e),
                code: crate::errors::ErrorCode::ConfigFileNotFound,
                source: Some(Box::new(e)),
            })?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| SnowbootError::Config {
                message: format!("Failed to parse config file: {}", e),
                code: crate::errors::ErrorCode::ConfigParseFailed,
                source: Some(Box::new(e)),
            })?;

        config.validate()?;
        Ok(config)
    }

    /// Apply environment variables to the configuration
    pub fn apply_env_vars(&mut self) {
        // Server configuration from environment
        if let Ok(host) = env::var("SNOWBOOT_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = env::var("SNOWBOOT_PORT") {
            if let Ok(p) = port.parse() {
                self.server.port = p;
            }
        }
        if let Ok(mount) = env::var("SNOWBOOT_MOUNT") {
            self.server.mount = mount;
        }
        if let Ok(user) = env::var("SNOWBOOT_USER") {
            self.server.username = user;
        }
        if let Ok(pass) = env::var("SNOWBOOT_PASSWORD") {
            self.server.password = pass;
        }
        if let Ok(tls) = env::var("SNOWBOOT_USE_TLS") {
            self.server.use_tls = tls.parse().unwrap_or(false);
        }

        // Audio configuration from environment
        if let Ok(rate) = env::var("SNOWBOOT_SAMPLE_RATE") {
            if let Ok(r) = rate.parse() {
                self.audio.sample_rate = r;
            }
        }
        if let Ok(bitrate) = env::var("SNOWBOOT_BITRATE") {
            if let Ok(b) = bitrate.parse() {
                self.audio.bitrate = b;
            }
        }
        if let Ok(buffer) = env::var("SNOWBOOT_BUFFER") {
            if let Ok(b) = buffer.parse() {
                self.audio.buffer_seconds = b;
            }
        }

        // Input configuration
        if let Ok(pipe) = env::var("SNOWBOOT_INPUT_PIPE") {
            self.input.pipe_path = pipe;
        }

        // Logging configuration
        if let Ok(level) = env::var("SNOWBOOT_LOG_LEVEL") {
            self.logging.level = level;
        }
        if let Ok(format) = env::var("SNOWBOOT_LOG_FORMAT") {
            if format == "json" {
                self.logging.format = LogFormat::Json;
            }
        }

        // Monitoring configuration
        if let Ok(enabled) = env::var("SNOWBOOT_METRICS_ENABLED") {
            self.monitoring.metrics_enabled = enabled.parse().unwrap_or(false);
        }
        if let Ok(port) = env::var("SNOWBOOT_METRICS_PORT") {
            if let Ok(p) = port.parse() {
                self.monitoring.metrics_port = p;
            }
        }
        if let Ok(enabled) = env::var("SNOWBOOT_HEALTH_ENABLED") {
            self.monitoring.health_enabled = enabled.parse().unwrap_or(false);
        }
        if let Ok(port) = env::var("SNOWBOOT_HEALTH_PORT") {
            if let Ok(p) = port.parse() {
                self.monitoring.health_port = p;
            }
        }
    }

    /// Validate all configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate port numbers
        if self.server.port == 0 {
            return Err(SnowbootError::invalid_port("0"));
        }

        // Validate audio settings
        if self.audio.sample_rate < 8000 || self.audio.sample_rate > 192000 {
            return Err(SnowbootError::invalid_sample_rate(self.audio.sample_rate));
        }

        if self.audio.bitrate < 8 || self.audio.bitrate > 500 {
            return Err(SnowbootError::invalid_bitrate(self.audio.bitrate));
        }

        if self.audio.buffer_seconds < 0.1 || self.audio.buffer_seconds > 10.0 {
            return Err(SnowbootError::invalid_buffer_size(self.audio.buffer_seconds));
        }

        // Validate hostname
        if self.server.host.is_empty() {
            return Err(SnowbootError::invalid_host("empty hostname"));
        }

        // Validate monitoring ports don't conflict
        if self.monitoring.metrics_enabled && self.monitoring.health_enabled {
            if self.monitoring.metrics_port == self.monitoring.health_port {
                return Err(SnowbootError::Config {
                    message: "Metrics and health ports cannot be the same".to_string(),
                    code: crate::errors::ErrorCode::InvalidPort,
                    source: None,
                });
            }
        }

        Ok(())
    }

    /// Generate an example TOML configuration file
    pub fn example_toml() -> String {
        r#"# Snowboot Configuration File

[server]
host = "localhost"
port = 8000
mount = "/stream.ogg"
username = "source"
# password = "your-password-here"  # Better to use SNOWBOOT_PASSWORD env var
use_tls = false

[audio]
sample_rate = 44100  # Hz (8000-192000, common: 44100, 48000)
bitrate = 320        # kbps (8-500)
buffer_seconds = 1.0 # seconds (0.1-10.0)

[input]
pipe_path = "/tmp/snowboot.in"

[logging]
level = "info"       # trace, debug, info, warn, error
format = "text"      # text or json

[monitoring]
metrics_enabled = false
metrics_port = 9090
health_enabled = false
health_port = 8080
"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 8000);
        assert_eq!(config.audio.sample_rate, 44100);
    }

    #[test]
    fn test_validation_invalid_sample_rate() {
        let mut config = Config::default();
        config.audio.sample_rate = 500000; // Too high
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_bitrate() {
        let mut config = Config::default();
        config.audio.bitrate = 1000; // Too high
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_buffer() {
        let mut config = Config::default();
        config.audio.buffer_seconds = 20.0; // Too large
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_load_from_toml() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
[server]
host = "example.com"
port = 8080

[audio]
sample_rate = 48000
bitrate = 192
        "#).unwrap();

        let config = Config::from_file(file.path()).unwrap();
        assert_eq!(config.server.host, "example.com");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.audio.sample_rate, 48000);
    }

    #[test]
    fn test_env_var_override() {
        env::set_var("SNOWBOOT_HOST", "testhost");
        env::set_var("SNOWBOOT_PORT", "9000");

        let mut config = Config::default();
        config.apply_env_vars();

        assert_eq!(config.server.host, "testhost");
        assert_eq!(config.server.port, 9000);

        env::remove_var("SNOWBOOT_HOST");
        env::remove_var("SNOWBOOT_PORT");
    }
}
