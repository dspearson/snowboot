// src/validation/validators.rs
//
// Validation functions for command-line arguments

use anyhow::{anyhow, Result};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

/// Validate a mount point string
pub fn validate_mount_point(s: &str) -> Result<String, String> {
    if !s.starts_with('/') {
        return Err("Mount point must start with a '/'".to_string());
    }
    Ok(s.to_string())
}

/// Validate an input pipe/file path
pub fn validate_input_pipe(s: &str) -> Result<String, String> {
    let path = Path::new(s);
    if path.exists() {
        // Check if it's a pipe or regular file
        match path.metadata() {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                if !(file_type.is_fifo() || file_type.is_file()) {
                    return Err("Input must be a pipe (FIFO) or regular file".to_string());
                }
            }
            Err(e) => return Err(format!("Cannot access input pipe: {}", e)),
        }
    }
    Ok(s.to_string())
}

/// Validate a log level string
pub fn validate_log_level(s: &str) -> Result<String, String> {
    match s {
        "trace" | "debug" | "info" | "warn" | "error" => Ok(s.to_string()),
        _ => Err("Log level must be one of: trace, debug, info, warn, error".to_string()),
    }
}

/// Validate a positive number
pub fn validate_positive_number(s: &str) -> Result<u64, String> {
    match s.parse::<u64>() {
        Ok(n) => Ok(n),
        Err(_) => Err("Value must be a positive number".to_string()),
    }
}

/// Validate host format (hostname or hostname:port)
pub fn validate_args(host: &str) -> Result<()> {
    // Check host format
    let host_parts: Vec<&str> = host.split(':').collect();
    if host_parts.len() > 2 {
        return Err(anyhow!(
            "Host format should be 'hostname' or 'hostname:port'"
        ));
    }

    if host_parts.len() == 2 {
        if let Err(_) = host_parts[1].parse::<u16>() {
            return Err(anyhow!("Port must be a valid number between 1-65535"));
        }
    }

    Ok(())
}
