// Integration tests for validation

use snowboot::validation::*;
use tempfile::tempdir;
use std::process::Command;

#[test]
fn test_valid_hostname() {
    assert!(validate_hostname("localhost").is_ok());
    assert!(validate_hostname("example.com").is_ok());
    assert!(validate_hostname("sub.domain.example.com").is_ok());
    assert!(validate_hostname("192.168.1.1").is_ok());
}

#[test]
fn test_invalid_hostname() {
    assert!(validate_hostname("").is_err());
    assert!(validate_hostname("test\0evil").is_err());
    assert!(validate_hostname("test\nline").is_err());
    assert!(validate_hostname(&"a".repeat(300)).is_err());
}

#[test]
fn test_valid_ports() {
    assert!(validate_port(80).is_ok());
    assert!(validate_port(8000).is_ok());
    assert!(validate_port(65535).is_ok());
    assert!(validate_port(1).is_ok());
}

#[test]
fn test_invalid_ports() {
    assert!(validate_port(0).is_err());
}

#[test]
fn test_sample_rate_bounds() {
    assert!(validate_sample_rate(8000).is_ok());
    assert!(validate_sample_rate(44100).is_ok());
    assert!(validate_sample_rate(48000).is_ok());
    assert!(validate_sample_rate(192000).is_ok());

    assert!(validate_sample_rate(7999).is_err());
    assert!(validate_sample_rate(192001).is_err());
}

#[test]
fn test_bitrate_bounds() {
    assert!(validate_bitrate(8).is_ok());
    assert!(validate_bitrate(128).is_ok());
    assert!(validate_bitrate(320).is_ok());
    assert!(validate_bitrate(500).is_ok());

    assert!(validate_bitrate(7).is_err());
    assert!(validate_bitrate(501).is_err());
}

#[test]
fn test_buffer_size_bounds() {
    assert!(validate_buffer_size(0.1).is_ok());
    assert!(validate_buffer_size(1.0).is_ok());
    assert!(validate_buffer_size(10.0).is_ok());

    assert!(validate_buffer_size(0.09).is_err());
    assert!(validate_buffer_size(10.1).is_err());
}

#[test]
fn test_parse_host_port() {
    let (host, port) = parse_host_port("localhost").unwrap();
    assert_eq!(host, "localhost");
    assert_eq!(port, 8000);

    let (host, port) = parse_host_port("example.com:8080").unwrap();
    assert_eq!(host, "example.com");
    assert_eq!(port, 8080);

    assert!(parse_host_port("invalid:port:format").is_err());
    assert!(parse_host_port("test:99999").is_err());
    assert!(parse_host_port("test:0").is_err());
}

#[test]
fn test_fifo_validation() {
    let dir = tempdir().unwrap();
    let fifo_path = dir.path().join("test.fifo");

    // Create a FIFO
    Command::new("mkfifo")
        .arg(&fifo_path)
        .output()
        .expect("Failed to create FIFO");

    // Should validate successfully
    assert!(validate_fifo(fifo_path.to_str().unwrap()).is_ok());

    // Test with regular file
    let file_path = dir.path().join("regular.txt");
    std::fs::write(&file_path, "test").unwrap();
    assert!(validate_fifo(file_path.to_str().unwrap()).is_err());

    // Test with non-existent path
    assert!(validate_fifo("/nonexistent/path").is_err());
}
