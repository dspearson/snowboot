// src/util/config.rs
//
// Configuration utilities

use crate::icecast::IcecastConfig;

/// Parse host and port from a string in the format "host" or "host:port"
pub fn parse_host_port(host_str: &str) -> (String, u16) {
    let parts: Vec<&str> = host_str.split(':').collect();
    let host = parts[0].to_string();
    let port = if parts.len() > 1 {
        parts[1].parse::<u16>().unwrap_or(8000)
    } else {
        8000 // Default Icecast port
    };

    (host, port)
}

/// Create an Icecast configuration from parameters
pub fn create_icecast_config(
    host_str: &str,
    mount: &str,
    username: &str,
    password: &str,
) -> IcecastConfig {
    let (host, port) = parse_host_port(host_str);

    IcecastConfig {
        host,
        port,
        mount: mount.to_string(),
        username: username.to_string(),
        password: password.to_string(),
    }
}
