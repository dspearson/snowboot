// Integration tests for configuration loading and validation

use snowboot::config::{Config, ServerConfig, AudioConfig};
use std::env;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_default_config_is_valid() {
    let config = Config::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_toml_config_loading() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, r#"
[server]
host = "streaming.example.com"
port = 8080
mount = "/live.ogg"
username = "broadcaster"
password = "supersecret"
use_tls = true

[audio]
sample_rate = 48000
bitrate = 192
buffer_seconds = 2.0

[input]
pipe_path = "/var/run/snowboot.fifo"

[logging]
level = "debug"
format = "json"

[monitoring]
metrics_enabled = true
metrics_port = 9091
health_enabled = true
health_port = 8081
    "#).unwrap();

    let config = Config::from_file(file.path()).unwrap();

    // Verify server config
    assert_eq!(config.server.host, "streaming.example.com");
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.mount, "/live.ogg");
    assert_eq!(config.server.username, "broadcaster");
    assert_eq!(config.server.password, "supersecret");
    assert_eq!(config.server.use_tls, true);

    // Verify audio config
    assert_eq!(config.audio.sample_rate, 48000);
    assert_eq!(config.audio.bitrate, 192);
    assert_eq!(config.audio.buffer_seconds, 2.0);

    // Verify input config
    assert_eq!(config.input.pipe_path, "/var/run/snowboot.fifo");

    // Verify logging config
    assert_eq!(config.logging.level, "debug");

    // Verify monitoring config
    assert_eq!(config.monitoring.metrics_enabled, true);
    assert_eq!(config.monitoring.metrics_port, 9091);
}

#[test]
fn test_env_var_overrides() {
    env::set_var("SNOWBOOT_HOST", "env.example.com");
    env::set_var("SNOWBOOT_PORT", "9999");
    env::set_var("SNOWBOOT_PASSWORD", "env_password");
    env::set_var("SNOWBOOT_SAMPLE_RATE", "96000");

    let mut config = Config::default();
    config.apply_env_vars();

    assert_eq!(config.server.host, "env.example.com");
    assert_eq!(config.server.port, 9999);
    assert_eq!(config.server.password, "env_password");
    assert_eq!(config.audio.sample_rate, 96000);

    // Cleanup
    env::remove_var("SNOWBOOT_HOST");
    env::remove_var("SNOWBOOT_PORT");
    env::remove_var("SNOWBOOT_PASSWORD");
    env::remove_var("SNOWBOOT_SAMPLE_RATE");
}

#[test]
fn test_invalid_sample_rate_rejected() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, r#"
[audio]
sample_rate = 500000
    "#).unwrap();

    let result = Config::from_file(file.path());
    assert!(result.is_err());
}

#[test]
fn test_invalid_bitrate_rejected() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, r#"
[audio]
bitrate = 1000
    "#).unwrap();

    let result = Config::from_file(file.path());
    assert!(result.is_err());
}

#[test]
fn test_port_conflict_detection() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, r#"
[monitoring]
metrics_enabled = true
metrics_port = 9090
health_enabled = true
health_port = 9090
    "#).unwrap();

    let result = Config::from_file(file.path());
    assert!(result.is_err());
}

#[test]
fn test_partial_config_uses_defaults() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, r#"
[server]
host = "partial.example.com"
    "#).unwrap();

    let config = Config::from_file(file.path()).unwrap();

    assert_eq!(config.server.host, "partial.example.com");
    assert_eq!(config.server.port, 8000); // Default
    assert_eq!(config.audio.sample_rate, 44100); // Default
}
