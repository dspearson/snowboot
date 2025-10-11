// HTTP server for health checks and metrics

use axum::{
    routing::get,
    Router,
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpListener;
use tracing::{info, error};

use crate::metrics::{get_metrics, HealthStatus};
use crate::connection::ConnectionState;

/// Shared state for HTTP server
#[derive(Clone)]
pub struct ServerState {
    pub start_time: std::time::Instant,
    pub connection_state: Arc<std::sync::Mutex<ConnectionState>>,
}

/// Start the metrics server
pub async fn start_metrics_server(
    port: u16,
    state: ServerState,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting metrics server on {}", addr);

    let listener = TcpListener::bind(addr).await?;

    // Run server until shutdown
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                error!("Metrics server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down metrics server");
        }
        _ = async {
            while running.load(Ordering::SeqCst) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        } => {
            info!("Metrics server stopping");
        }
    }

    Ok(())
}

/// Start the health check server
pub async fn start_health_server(
    port: u16,
    state: ServerState,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting health server on {}", addr);

    let listener = TcpListener::bind(addr).await?;

    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                error!("Health server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down health server");
        }
        _ = async {
            while running.load(Ordering::SeqCst) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        } => {
            info!("Health server stopping");
        }
    }

    Ok(())
}

/// Metrics endpoint handler
async fn metrics_handler() -> impl IntoResponse {
    let metrics = get_metrics();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(metrics)
        .unwrap()
}

/// Health endpoint handler
async fn health_handler(
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    let connection_state = state.connection_state.lock().unwrap();

    let connection_state_str = match *connection_state {
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Connecting => "connecting",
        ConnectionState::Connected => "connected",
        ConnectionState::Reconnecting => "reconnecting",
        ConnectionState::Failed => "failed",
    };

    let health = HealthStatus::new(connection_state_str, uptime);

    let status_code = if health.status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(health))
}

/// Readiness endpoint handler
async fn ready_handler(
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> impl IntoResponse {
    let connection_state = state.connection_state.lock().unwrap();

    match *connection_state {
        ConnectionState::Connected => {
            (StatusCode::OK, Json(serde_json::json!({
                "ready": true,
                "status": "connected"
            })))
        }
        _ => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
                "ready": false,
                "status": format!("{:?}", *connection_state).to_lowercase()
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_creation() {
        let state = ServerState {
            start_time: std::time::Instant::now(),
            connection_state: Arc::new(std::sync::Mutex::new(ConnectionState::Connected)),
        };

        let cs = state.connection_state.lock().unwrap();
        assert_eq!(*cs, ConnectionState::Connected);
    }
}
