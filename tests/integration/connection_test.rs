// Integration tests for connection manager

use snowboot::connection::{ConnectionManager, RetryConfig};
use snowboot::icecast::IcecastConfig;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;

#[tokio::test]
async fn test_connection_manager_retry_on_failure() {
    let retry_config = RetryConfig {
        max_retries: 3,
        initial_backoff_secs: 0.1,
        max_backoff_secs: 1.0,
        backoff_multiplier: 2.0,
        connection_timeout_secs: 2,
    };

    let icecast_config = IcecastConfig {
        host: "nonexistent.invalid".to_string(),
        port: 9999,
        mount: "/test.ogg".to_string(),
        username: "test".to_string(),
        password: "test".to_string(),
        content_type: "application/ogg".to_string(),
    };

    let manager = ConnectionManager::new(icecast_config, retry_config);

    // This should fail after 3 retries
    let result = manager.connect().await;
    assert!(result.is_err());
    assert_eq!(manager.retry_count(), 3);
}

#[tokio::test]
async fn test_connection_manager_success() {
    // Start a mock server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn mock server
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;

            // Send HTTP 100 Continue response
            let response = b"HTTP/1.1 100 Continue\r\n\r\n";
            let _ = socket.write_all(response).await;
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    let retry_config = RetryConfig {
        max_retries: 3,
        initial_backoff_secs: 0.1,
        max_backoff_secs: 1.0,
        backoff_multiplier: 2.0,
        connection_timeout_secs: 2,
    };

    let icecast_config = IcecastConfig {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
        mount: "/test.ogg".to_string(),
        username: "test".to_string(),
        password: "test".to_string(),
        content_type: "application/ogg".to_string(),
    };

    let manager = ConnectionManager::new(icecast_config, retry_config);

    // This should succeed
    let result = manager.connect().await;
    assert!(result.is_ok());
    assert!(manager.is_connected());
}

#[tokio::test]
async fn test_connection_manager_auth_failure_no_retry() {
    // Start a mock server that sends 401
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn mock server
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;

            // Send HTTP 401 Unauthorized response
            let response = b"HTTP/1.1 401 Unauthorized\r\n\r\n";
            let _ = socket.write_all(response).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let retry_config = RetryConfig {
        max_retries: 5, // Even with 5 retries allowed
        initial_backoff_secs: 0.1,
        max_backoff_secs: 1.0,
        backoff_multiplier: 2.0,
        connection_timeout_secs: 2,
    };

    let icecast_config = IcecastConfig {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
        mount: "/test.ogg".to_string(),
        username: "test".to_string(),
        password: "wrong".to_string(),
        content_type: "application/ogg".to_string(),
    };

    let manager = ConnectionManager::new(icecast_config, retry_config);

    // Should fail immediately without retries (auth error)
    let result = manager.connect().await;
    assert!(result.is_err());
    assert_eq!(manager.retry_count(), 0); // No retries for auth failure
}

#[tokio::test]
async fn test_exponential_backoff() {
    let retry_config = RetryConfig {
        max_retries: 0, // Infinite
        initial_backoff_secs: 1.0,
        max_backoff_secs: 60.0,
        backoff_multiplier: 2.0,
        connection_timeout_secs: 1,
    };

    // Calculate expected backoffs
    let mut backoff = retry_config.initial_backoff_secs;
    let expected_backoffs = vec![
        1.0,  // Initial
        2.0,  // 1.0 * 2
        4.0,  // 2.0 * 2
        8.0,  // 4.0 * 2
        16.0, // 8.0 * 2
        32.0, // 16.0 * 2
        60.0, // 32.0 * 2 = 64, capped at 60
        60.0, // Stays at max
    ];

    for expected in expected_backoffs {
        backoff = (backoff * retry_config.backoff_multiplier).min(retry_config.max_backoff_secs);
        assert_eq!(backoff, expected);
    }
}
