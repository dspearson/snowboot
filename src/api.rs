use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::connection::ConnectionState;
use crate::metrics::{self, get_metrics, HealthStatus};
use crate::player::PlayerHandle;
use crate::queue::{SharedQueue, Track};

#[derive(Clone)]
pub struct AppState {
    pub queue: SharedQueue,
    pub player: PlayerHandle,
    pub start_time: Instant,
    pub connection_state: Arc<std::sync::Mutex<ConnectionState>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/queue", get(list_queue))
        .route("/api/queue", post(add_track))
        .route("/api/queue", delete(clear_queue))
        .route("/api/queue/next", post(add_track_next))
        .route("/api/queue/{id}", delete(remove_track))
        .route("/api/queue/{id}/position", put(move_track))
        .route("/api/skip", post(skip_track))
        .route("/api/status", get(status))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

// --- Request/Response types ---

#[derive(Deserialize)]
struct AddTrackRequest {
    path: String,
}

#[derive(Deserialize)]
struct MoveTrackRequest {
    position: usize,
}

#[derive(Serialize)]
struct StatusResponse {
    now_playing: Option<Track>,
    queue_length: usize,
    connection_state: String,
    uptime_seconds: u64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: u32,
}

fn error_response(status: StatusCode, msg: &str, code: u32) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg.to_string(), code }))
}

fn validate_ogg_file(path: &str) -> Result<PathBuf, (StatusCode, Json<ErrorResponse>)> {
    let path_buf = PathBuf::from(path);

    if !path_buf.exists() {
        return Err(error_response(StatusCode::NOT_FOUND, "File not found", 3010));
    }

    // Check readable
    if std::fs::File::open(&path_buf).is_err() {
        return Err(error_response(StatusCode::BAD_REQUEST, "File not readable", 3011));
    }

    // Check extension
    match path_buf.extension().and_then(|e| e.to_str()) {
        Some("ogg") | Some("oga") => {}
        _ => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "File must have .ogg or .oga extension",
                3012,
            ));
        }
    }

    Ok(path_buf)
}

// --- Handlers ---

async fn list_queue(State(state): State<AppState>) -> impl IntoResponse {
    let q = state.queue.read().await;
    Json(q.list())
}

async fn add_track(
    State(state): State<AppState>,
    Json(req): Json<AddTrackRequest>,
) -> Result<(StatusCode, Json<Track>), (StatusCode, Json<ErrorResponse>)> {
    let path_buf = validate_ogg_file(&req.path)?;
    let track = Track::from_file(path_buf);
    let response = track.clone();

    {
        let mut q = state.queue.write().await;
        q.push_back(track);
        metrics::QUEUE_LENGTH.set(q.len() as i64);
    }

    Ok((StatusCode::CREATED, Json(response)))
}

async fn add_track_next(
    State(state): State<AppState>,
    Json(req): Json<AddTrackRequest>,
) -> Result<(StatusCode, Json<Track>), (StatusCode, Json<ErrorResponse>)> {
    let path_buf = validate_ogg_file(&req.path)?;
    let track = Track::from_file(path_buf);
    let response = track.clone();

    {
        let mut q = state.queue.write().await;
        q.push_front(track);
        metrics::QUEUE_LENGTH.set(q.len() as i64);
    }

    Ok((StatusCode::CREATED, Json(response)))
}

async fn remove_track(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Track>, (StatusCode, Json<ErrorResponse>)> {
    let mut q = state.queue.write().await;
    match q.remove(id) {
        Some(track) => {
            metrics::QUEUE_LENGTH.set(q.len() as i64);
            Ok(Json(track))
        }
        None => Err(error_response(StatusCode::NOT_FOUND, "Track not found", 6001)),
    }
}

async fn move_track(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<MoveTrackRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut q = state.queue.write().await;
    if q.move_track(id, req.position) {
        Ok(StatusCode::OK)
    } else {
        Err(error_response(StatusCode::NOT_FOUND, "Track not found", 6001))
    }
}

async fn clear_queue(State(state): State<AppState>) -> StatusCode {
    let mut q = state.queue.write().await;
    q.clear();
    metrics::QUEUE_LENGTH.set(0);
    StatusCode::NO_CONTENT
}

async fn skip_track(State(state): State<AppState>) -> StatusCode {
    state.player.skip().await;
    StatusCode::OK
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let now_playing = state.player.now_playing();
    let queue_length = state.queue.read().await.len();
    let uptime = state.start_time.elapsed().as_secs();

    let cs = state.connection_state.lock().unwrap();
    let connection_state = format!("{:?}", *cs).to_lowercase();

    Json(StatusResponse {
        now_playing,
        queue_length,
        connection_state,
        uptime_seconds: uptime,
    })
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();
    let cs = state.connection_state.lock().unwrap();
    let cs_str = match *cs {
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Connecting => "connecting",
        ConnectionState::Connected => "connected",
        ConnectionState::Reconnecting => "reconnecting",
        ConnectionState::Failed => "failed",
    };

    let health = HealthStatus::new(cs_str, uptime);
    let status_code = if health.status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(health))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let cs = state.connection_state.lock().unwrap();
    match *cs {
        ConnectionState::Connected => {
            (StatusCode::OK, Json(serde_json::json!({
                "ready": true,
                "status": "connected"
            })))
        }
        _ => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
                "ready": false,
                "status": format!("{:?}", *cs).to_lowercase()
            })))
        }
    }
}

async fn metrics_handler() -> impl IntoResponse {
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(get_metrics())
        .unwrap()
}
