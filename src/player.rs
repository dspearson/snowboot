use std::sync::Arc;
use bytes::Bytes;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::metrics;
use crate::queue::{SharedQueue, Track};

#[derive(Clone)]
pub struct PlayerHandle {
    skip_token: Arc<RwLock<CancellationToken>>,
    pub queue: SharedQueue,
    now_playing: Arc<std::sync::RwLock<Option<Track>>>,
}

impl PlayerHandle {
    pub fn new(queue: SharedQueue) -> Self {
        Self {
            skip_token: Arc::new(RwLock::new(CancellationToken::new())),
            queue,
            now_playing: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub async fn skip(&self) {
        let token = self.skip_token.read().await;
        token.cancel();
        info!("Skip requested");
    }

    pub fn now_playing(&self) -> Option<Track> {
        self.now_playing.read().unwrap().clone()
    }
}

pub async fn run_player(
    handle: PlayerHandle,
    input_tx: mpsc::Sender<Bytes>,
    shutdown: CancellationToken,
) {
    info!("Player task started");

    loop {
        if shutdown.is_cancelled() {
            break;
        }

        let track = {
            let mut q = handle.queue.write().await;
            q.pop_front()
        };

        let track = match track {
            Some(t) => t,
            None => {
                // Update queue length metric
                metrics::QUEUE_LENGTH.set(0);
                sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        // Update queue length metric after pop
        {
            let q = handle.queue.read().await;
            metrics::QUEUE_LENGTH.set(q.len() as i64);
        }

        info!("Now playing: {} ({})", track.title, track.path.display());

        // Set now_playing
        *handle.now_playing.write().unwrap() = Some(track.clone());

        // Create a fresh cancellation token for this track
        let track_token = CancellationToken::new();
        *handle.skip_token.write().await = track_token.clone();

        // Stream the file
        let was_skipped = stream_file(&track.path, &input_tx, &track_token, &shutdown).await;

        if was_skipped {
            metrics::TRACKS_SKIPPED.inc();
            info!("Skipped: {}", track.title);
        }

        metrics::TRACKS_PLAYED.inc();

        // Clear now_playing
        *handle.now_playing.write().unwrap() = None;
    }

    debug!("Player task finished");
}

async fn stream_file(
    path: &std::path::Path,
    input_tx: &mpsc::Sender<Bytes>,
    skip_token: &CancellationToken,
    shutdown: &CancellationToken,
) -> bool {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open file {}: {}", path.display(), e);
            return false;
        }
    };

    let mut buf = [0u8; 8192];

    loop {
        tokio::select! {
            _ = skip_token.cancelled() => {
                return true;
            }
            _ = shutdown.cancelled() => {
                return false;
            }
            result = file.read(&mut buf) => {
                match result {
                    Ok(0) => return false, // EOF
                    Ok(n) => {
                        let data = Bytes::copy_from_slice(&buf[..n]);
                        if input_tx.send(data).await.is_err() {
                            warn!("oggmux channel closed");
                            return false;
                        }
                    }
                    Err(e) => {
                        error!("Error reading file {}: {}", path.display(), e);
                        return false;
                    }
                }
            }
        }
    }
}
