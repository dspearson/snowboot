use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use snowboot::api::{AppState, router};
use snowboot::connection::ConnectionState;
use snowboot::player::PlayerHandle;
use snowboot::queue::{Queue, SharedQueue};

fn test_state() -> AppState {
    let queue: SharedQueue = Arc::new(tokio::sync::RwLock::new(Queue::default()));
    let player = PlayerHandle::new(queue.clone());
    AppState {
        queue,
        player,
        start_time: Instant::now(),
        connection_state: Arc::new(std::sync::Mutex::new(ConnectionState::Connected)),
        media_dir: None,
        api_token: None,
    }
}

fn test_state_with_auth(token: &str) -> AppState {
    let mut state = test_state();
    state.api_token = Some(token.to_string());
    state
}

#[tokio::test]
async fn test_health_endpoint() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ready_endpoint() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_status_endpoint() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_empty_queue() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/api/queue").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let tracks: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(tracks.is_empty());
}

#[tokio::test]
async fn test_add_track_file_not_found() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::post("/api/queue")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path": "/nonexistent/file.ogg"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_remove_nonexistent_track() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::delete("/api/queue/999").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_clear_queue() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::delete("/api/queue").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_skip() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::post("/api/skip").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_shuffle_empty_queue() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::post("/api/queue/shuffle").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_history_empty() {
    let app = router(test_state());
    let resp = app
        .oneshot(Request::get("/api/history").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let history: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(history.is_empty());
}

// --- Auth tests ---

#[tokio::test]
async fn test_auth_required_no_token() {
    let app = router(test_state_with_auth("secret123"));
    let resp = app
        .oneshot(Request::get("/api/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_required_wrong_token() {
    let app = router(test_state_with_auth("secret123"));
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_correct_token() {
    let app = router(test_state_with_auth("secret123"));
    let resp = app
        .oneshot(
            Request::get("/api/status")
                .header("authorization", "Bearer secret123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_bypasses_auth() {
    let app = router(test_state_with_auth("secret123"));
    let resp = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_bypasses_auth() {
    let app = router(test_state_with_auth("secret123"));
    let resp = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- Media dir tests ---

#[tokio::test]
async fn test_media_dir_blocks_outside_paths() {
    let mut state = test_state();
    state.media_dir = Some(std::path::PathBuf::from("/tmp/snowboot-test-media"));

    let app = router(state);
    let resp = app
        .oneshot(
            Request::post("/api/queue")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path": "/etc/passwd"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // Rejected — should not succeed
    assert_ne!(resp.status(), StatusCode::OK, "Should reject path outside media dir");
    assert_ne!(resp.status(), StatusCode::CREATED, "Should reject path outside media dir");
}
