// Input validation and safety checks

use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use crate::errors::{Result, SnowbootError};

/// Validate a port number
pub fn validate_port(port: u16) -> Result<()> {
    if port == 0 {
        return Err(SnowbootError::invalid_port("0"));
    }
    Ok(())
}

/// Validate a hostname
pub fn validate_hostname(host: &str) -> Result<()> {
    if host.is_empty() {
        return Err(SnowbootError::invalid_host("empty hostname"));
    }

    // Check for obviously invalid characters
    if host.contains('\0') || host.contains('\n') || host.contains('\r') {
        return Err(SnowbootError::invalid_host(host));
    }

    // Basic length check (DNS limit is 253 chars)
    if host.len() > 253 {
        return Err(SnowbootError::invalid_host("hostname too long (max 253 characters)"));
    }

    Ok(())
}

/// Validate sample rate
pub fn validate_sample_rate(rate: u32) -> Result<()> {
    const MIN_RATE: u32 = 8000;
    const MAX_RATE: u32 = 192000;

    if rate < MIN_RATE || rate > MAX_RATE {
        return Err(SnowbootError::invalid_sample_rate(rate));
    }

    Ok(())
}

/// Validate bitrate
pub fn validate_bitrate(bitrate: u32) -> Result<()> {
    const MIN_BITRATE: u32 = 8;
    const MAX_BITRATE: u32 = 500;

    if bitrate < MIN_BITRATE || bitrate > MAX_BITRATE {
        return Err(SnowbootError::invalid_bitrate(bitrate));
    }

    Ok(())
}

/// Validate buffer size
pub fn validate_buffer_size(size: f64) -> Result<()> {
    const MIN_BUFFER: f64 = 0.1;
    const MAX_BUFFER: f64 = 10.0;

    if size < MIN_BUFFER || size > MAX_BUFFER {
        return Err(SnowbootError::invalid_buffer_size(size));
    }

    Ok(())
}

/// Validate and check if a path is a FIFO (named pipe)
pub fn validate_fifo(path: &str) -> Result<()> {
    let path_obj = Path::new(path);

    // Check if path exists
    if !path_obj.exists() {
        return Err(SnowbootError::pipe_not_found(path));
    }

    // Check if it's a FIFO
    let metadata = fs::metadata(path_obj)
        .map_err(|e| SnowbootError::pipe_open_failed(path, e))?;

    if !metadata.file_type().is_fifo() {
        return Err(SnowbootError::not_a_fifo(path));
    }

    // Check permissions - we need read access
    let permissions = metadata.permissions();
    if permissions.readonly() {
        return Err(SnowbootError::Io {
            message: format!("FIFO is read-only: {}", path),
            code: crate::errors::ErrorCode::PermissionDenied,
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only"),
        });
    }

    Ok(())
}

/// Parse and validate host:port string
pub fn parse_host_port(host_str: &str) -> Result<(String, u16)> {
    let parts: Vec<&str> = host_str.split(':').collect();

    match parts.len() {
        1 => {
            validate_hostname(parts[0])?;
            Ok((parts[0].to_string(), 8000))
        }
        2 => {
            validate_hostname(parts[0])?;
            let port = parts[1].parse::<u16>()
                .map_err(|_| SnowbootError::invalid_port(parts[1]))?;
            validate_port(port)?;
            Ok((parts[0].to_string(), port))
        }
        _ => Err(SnowbootError::invalid_host(host_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    use tempfile::tempdir;
    use std::process::Command;

    #[test]
    fn test_validate_port() {
        assert!(validate_port(0).is_err());
        assert!(validate_port(80).is_ok());
        assert!(validate_port(65535).is_ok());
    }

    #[test]
    fn test_validate_hostname() {
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("localhost").is_ok());
        assert!(validate_hostname("example.com").is_ok());
        assert!(validate_hostname("test\0evil").is_err());
        assert!(validate_hostname(&"a".repeat(300)).is_err());
    }

    #[test]
    fn test_validate_sample_rate() {
        assert!(validate_sample_rate(7999).is_err());
        assert!(validate_sample_rate(8000).is_ok());
        assert!(validate_sample_rate(44100).is_ok());
        assert!(validate_sample_rate(192000).is_ok());
        assert!(validate_sample_rate(192001).is_err());
    }

    #[test]
    fn test_validate_bitrate() {
        assert!(validate_bitrate(7).is_err());
        assert!(validate_bitrate(8).is_ok());
        assert!(validate_bitrate(320).is_ok());
        assert!(validate_bitrate(500).is_ok());
        assert!(validate_bitrate(501).is_err());
    }

    #[test]
    fn test_validate_buffer_size() {
        assert!(validate_buffer_size(0.05).is_err());
        assert!(validate_buffer_size(0.1).is_ok());
        assert!(validate_buffer_size(1.0).is_ok());
        assert!(validate_buffer_size(10.0).is_ok());
        assert!(validate_buffer_size(10.1).is_err());
    }

    #[test]
    fn test_parse_host_port() {
        let (host, port) = parse_host_port("localhost").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 8000);

        let (host, port) = parse_host_port("example.com:9000").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 9000);

        assert!(parse_host_port("invalid:port:format").is_err());
        assert!(parse_host_port("test:99999").is_err());
    }

    #[test]
    fn test_validate_fifo() {
        let dir = tempdir().unwrap();
        let fifo_path = dir.path().join("test.fifo");

        // Create a FIFO
        Command::new("mkfifo")
            .arg(&fifo_path)
            .output()
            .expect("Failed to create FIFO");

        assert!(validate_fifo(fifo_path.to_str().unwrap()).is_ok());

        // Test with regular file
        let file_path = dir.path().join("regular.txt");
        std::fs::write(&file_path, "test").unwrap();
        assert!(validate_fifo(file_path.to_str().unwrap()).is_err());

        // Test with non-existent path
        assert!(validate_fifo("/nonexistent/path").is_err());
    }
}
