use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::connection::ConnectionState;
use crate::metrics::{self, get_metrics, HealthStatus};
use crate::player::{PlayerEvent, PlayerHandle};
use crate::queue::{SharedQueue, Track};

#[derive(Clone)]
pub struct AppState {
    pub queue: SharedQueue,
    pub player: PlayerHandle,
    pub start_time: Instant,
    pub connection_state: Arc<std::sync::Mutex<ConnectionState>>,
    pub media_dir: Option<PathBuf>,
    pub api_token: Option<String>,
}

pub fn router(state: AppState) -> Router {
    let authed_api = Router::new()
        .route("/api/queue", get(list_queue))
        .route("/api/queue", post(add_track))
        .route("/api/queue", delete(clear_queue))
        .route("/api/queue/next", post(add_track_next))
        .route("/api/queue/bulk", post(add_tracks_bulk))
        .route("/api/queue/shuffle", post(shuffle_queue))
        .route("/api/queue/{id}", delete(remove_track))
        .route("/api/queue/{id}/position", put(move_track))
        .route("/api/skip", post(skip_track))
        .route("/api/status", get(status))
        .route("/api/history", get(history))
        .route("/api/events", get(events_sse))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    let public = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    authed_api.merge(public)
}

// --- Auth middleware ---

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(ref expected) = state.api_token {
        let authorised = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|token| token == expected)
            .unwrap_or(false);

        if !authorised {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}

// --- Request/Response types ---

#[derive(Deserialize)]
struct AddTrackRequest {
    path: String,
}

#[derive(Deserialize)]
struct BulkAddRequest {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    directory: Option<String>,
    #[serde(default)]
    recursive: bool,
}

#[derive(Serialize)]
struct BulkAddResponse {
    added: Vec<Track>,
    errors: Vec<String>,
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
    (
        status,
        Json(ErrorResponse {
            error: msg.to_string(),
            code,
        }),
    )
}

fn validate_ogg_file(
    path: &str,
    media_dir: &Option<PathBuf>,
) -> Result<PathBuf, (StatusCode, Json<ErrorResponse>)> {
    let path_buf = PathBuf::from(path);

    // Canonicalise and check media_dir restriction
    if let Some(ref media_dir) = media_dir {
        let canonical = path_buf
            .canonicalize()
            .map_err(|_| error_response(StatusCode::NOT_FOUND, "File not found", 3010))?;
        let media_canonical = media_dir
            .canonicalize()
            .map_err(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Media directory not accessible",
                    3013,
                )
            })?;
        if !canonical.starts_with(&media_canonical) {
            return Err(error_response(
                StatusCode::FORBIDDEN,
                "Path outside media directory",
                3014,
            ));
        }
    }

    if !path_buf.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "File not found",
            3010,
        ));
    }

    if std::fs::File::open(&path_buf).is_err() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "File not readable",
            3011,
        ));
    }

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

fn scan_directory(dir: &std::path::Path, recursive: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && recursive {
            files.extend(scan_directory(&path, true));
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "ogg" || ext == "oga" {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files
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
    let path_buf = validate_ogg_file(&req.path, &state.media_dir)?;
    let track = Track::from_file(path_buf);
    let response = track.clone();

    {
        let mut q = state.queue.write().await;
        q.push_back(track);
        metrics::QUEUE_LENGTH.set(q.len() as i64);
    }

    state.player.send_event(PlayerEvent::QueueChanged {
        length: state.queue.read().await.len(),
    });

    Ok((StatusCode::CREATED, Json(response)))
}

async fn add_track_next(
    State(state): State<AppState>,
    Json(req): Json<AddTrackRequest>,
) -> Result<(StatusCode, Json<Track>), (StatusCode, Json<ErrorResponse>)> {
    let path_buf = validate_ogg_file(&req.path, &state.media_dir)?;
    let track = Track::from_file(path_buf);
    let response = track.clone();

    {
        let mut q = state.queue.write().await;
        q.push_front(track);
        metrics::QUEUE_LENGTH.set(q.len() as i64);
    }

    state.player.send_event(PlayerEvent::QueueChanged {
        length: state.queue.read().await.len(),
    });

    Ok((StatusCode::CREATED, Json(response)))
}

async fn add_tracks_bulk(
    State(state): State<AppState>,
    Json(req): Json<BulkAddRequest>,
) -> (StatusCode, Json<BulkAddResponse>) {
    let mut added = Vec::new();
    let mut errors = Vec::new();

    // Collect paths from explicit list and directory scan
    let mut all_paths: Vec<String> = req.paths;

    if let Some(ref dir) = req.directory {
        let dir_path = PathBuf::from(dir);
        if dir_path.is_dir() {
            let scanned = scan_directory(&dir_path, req.recursive);
            for p in scanned {
                all_paths.push(p.to_string_lossy().to_string());
            }
        } else {
            errors.push(format!("{}: not a directory", dir));
        }
    }

    for path_str in &all_paths {
        match validate_ogg_file(path_str, &state.media_dir) {
            Ok(path_buf) => {
                let track = Track::from_file(path_buf);
                added.push(track.clone());
                state.queue.write().await.push_back(track);
            }
            Err((_, Json(err))) => {
                errors.push(format!("{}: {}", path_str, err.error));
            }
        }
    }

    {
        let q = state.queue.read().await;
        metrics::QUEUE_LENGTH.set(q.len() as i64);
    }

    if !added.is_empty() {
        state.player.send_event(PlayerEvent::QueueChanged {
            length: state.queue.read().await.len(),
        });
    }

    let status = if added.is_empty() && !errors.is_empty() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::CREATED
    };

    (status, Json(BulkAddResponse { added, errors }))
}

async fn shuffle_queue(State(state): State<AppState>) -> StatusCode {
    let mut q = state.queue.write().await;
    q.shuffle();
    state.player.send_event(PlayerEvent::QueueChanged {
        length: q.len(),
    });
    StatusCode::OK
}

async fn remove_track(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Track>, (StatusCode, Json<ErrorResponse>)> {
    let mut q = state.queue.write().await;
    match q.remove(id) {
        Some(track) => {
            metrics::QUEUE_LENGTH.set(q.len() as i64);
            state
                .player
                .send_event(PlayerEvent::QueueChanged { length: q.len() });
            Ok(Json(track))
        }
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "Track not found",
            6001,
        )),
    }
}

async fn move_track(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<MoveTrackRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut q = state.queue.write().await;
    if q.move_track(id, req.position) {
        state
            .player
            .send_event(PlayerEvent::QueueChanged { length: q.len() });
        Ok(StatusCode::OK)
    } else {
        Err(error_response(
            StatusCode::NOT_FOUND,
            "Track not found",
            6001,
        ))
    }
}

async fn clear_queue(State(state): State<AppState>) -> StatusCode {
    let mut q = state.queue.write().await;
    q.clear();
    metrics::QUEUE_LENGTH.set(0);
    state
        .player
        .send_event(PlayerEvent::QueueChanged { length: 0 });
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

async fn history(State(state): State<AppState>) -> impl IntoResponse {
    let history = state.player.history.read().unwrap();
    Json(history.clone())
}

async fn events_sse(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.player.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                let event_name = match &event {
                    PlayerEvent::TrackStarted(_) => "track_started",
                    PlayerEvent::TrackFinished { .. } => "track_finished",
                    PlayerEvent::TrackSkipped { .. } => "track_skipped",
                    PlayerEvent::QueueChanged { .. } => "queue_changed",
                };
                Some(Ok(Event::default().event(event_name).data(json)))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
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
        ConnectionState::Connected => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ready": true,
                "status": "connected"
            })),
        ),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "ready": false,
                "status": format!("{:?}", *cs).to_lowercase()
            })),
        ),
    }
}

async fn metrics_handler() -> impl IntoResponse {
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(get_metrics())
        .unwrap()
}
