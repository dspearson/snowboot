// Metrics and health check module

use prometheus::{
    IntCounter, IntGauge, Histogram, HistogramOpts, Opts, Registry,
    Encoder, TextEncoder,
};
use std::sync::Arc;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // Connection metrics
    pub static ref CONNECTION_ATTEMPTS: IntCounter = IntCounter::new(
        "snowboot_connection_attempts_total",
        "Total number of connection attempts"
    ).unwrap();

    pub static ref CONNECTION_FAILURES: IntCounter = IntCounter::new(
        "snowboot_connection_failures_total",
        "Total number of connection failures"
    ).unwrap();

    pub static ref CONNECTION_STATE: IntGauge = IntGauge::new(
        "snowboot_connection_state",
        "Current connection state (0=disconnected, 1=connecting, 2=connected, 3=reconnecting, 4=failed)"
    ).unwrap();

    pub static ref RECONNECT_COUNT: IntCounter = IntCounter::new(
        "snowboot_reconnect_total",
        "Total number of reconnection attempts"
    ).unwrap();

    // Data transfer metrics
    pub static ref BYTES_SENT: IntCounter = IntCounter::new(
        "snowboot_bytes_sent_total",
        "Total bytes sent to Icecast"
    ).unwrap();

    pub static ref BYTES_READ: IntCounter = IntCounter::new(
        "snowboot_bytes_read_total",
        "Total bytes read from input pipe"
    ).unwrap();

    pub static ref CHUNKS_SENT: IntCounter = IntCounter::new(
        "snowboot_chunks_sent_total",
        "Total chunks sent to Icecast"
    ).unwrap();

    // Performance metrics
    pub static ref SEND_DURATION: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "snowboot_send_duration_seconds",
            "Time to send data to Icecast"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
    ).unwrap();

    pub static ref BUFFER_SIZE: IntGauge = IntGauge::new(
        "snowboot_buffer_size_bytes",
        "Current buffer size in bytes"
    ).unwrap();

    // Error metrics
    pub static ref ERRORS_TOTAL: IntCounter = IntCounter::new(
        "snowboot_errors_total",
        "Total number of errors"
    ).unwrap();

    pub static ref PIPE_ERRORS: IntCounter = IntCounter::new(
        "snowboot_pipe_errors_total",
        "Total number of pipe read errors"
    ).unwrap();

    // Uptime metric
    pub static ref UPTIME_SECONDS: IntGauge = IntGauge::new(
        "snowboot_uptime_seconds",
        "Uptime in seconds"
    ).unwrap();
}

/// Initialize metrics registry
pub fn init_metrics() {
    // Register all metrics
    REGISTRY.register(Box::new(CONNECTION_ATTEMPTS.clone())).unwrap();
    REGISTRY.register(Box::new(CONNECTION_FAILURES.clone())).unwrap();
    REGISTRY.register(Box::new(CONNECTION_STATE.clone())).unwrap();
    REGISTRY.register(Box::new(RECONNECT_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(BYTES_SENT.clone())).unwrap();
    REGISTRY.register(Box::new(BYTES_READ.clone())).unwrap();
    REGISTRY.register(Box::new(CHUNKS_SENT.clone())).unwrap();
    REGISTRY.register(Box::new(SEND_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(BUFFER_SIZE.clone())).unwrap();
    REGISTRY.register(Box::new(ERRORS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(PIPE_ERRORS.clone())).unwrap();
    REGISTRY.register(Box::new(UPTIME_SECONDS.clone())).unwrap();
}

/// Get metrics as text in Prometheus format
pub fn get_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Health status
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub uptime_seconds: u64,
    pub connection_state: String,
    pub bytes_sent: u64,
    pub bytes_read: u64,
    pub errors: u64,
}

impl HealthStatus {
    pub fn new(
        connection_state: &str,
        uptime_seconds: u64,
    ) -> Self {
        Self {
            status: if connection_state == "connected" { "healthy".to_string() } else { "degraded".to_string() },
            uptime_seconds,
            connection_state: connection_state.to_string(),
            bytes_sent: BYTES_SENT.get(),
            bytes_read: BYTES_READ.get(),
            errors: ERRORS_TOTAL.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        init_metrics();

        // Increment a metric
        BYTES_SENT.inc_by(1024);
        assert_eq!(BYTES_SENT.get(), 1024);

        // Get metrics text
        let metrics_text = get_metrics();
        assert!(metrics_text.contains("snowboot_bytes_sent_total"));
    }

    #[test]
    fn test_health_status() {
        let health = HealthStatus::new("connected", 3600);
        assert_eq!(health.status, "healthy");
        assert_eq!(health.connection_state, "connected");
        assert_eq!(health.uptime_seconds, 3600);
    }
}
