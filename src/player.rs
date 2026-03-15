use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use serde::Serialize;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::metrics;
use crate::queue::{SharedQueue, Track};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum PlayerEvent {
    #[serde(rename = "track_started")]
    TrackStarted(Track),
    #[serde(rename = "track_finished")]
    TrackFinished { track: Track, duration_secs: u64 },
    #[serde(rename = "track_skipped")]
    TrackSkipped { track: Track, duration_secs: u64 },
    #[serde(rename = "queue_changed")]
    QueueChanged { length: usize },
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub track: Track,
    pub started_at: u64,
    pub duration_secs: u64,
    pub skipped: bool,
}

pub type SharedHistory = Arc<std::sync::RwLock<Vec<HistoryEntry>>>;

#[derive(Clone)]
pub struct PlayerHandle {
    skip_token: Arc<RwLock<CancellationToken>>,
    pub queue: SharedQueue,
    now_playing: Arc<std::sync::RwLock<Option<Track>>>,
    pub event_tx: broadcast::Sender<PlayerEvent>,
    pub history: SharedHistory,
}

impl PlayerHandle {
    pub fn new(queue: SharedQueue) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            skip_token: Arc::new(RwLock::new(CancellationToken::new())),
            queue,
            now_playing: Arc::new(std::sync::RwLock::new(None)),
            event_tx,
            history: Arc::new(std::sync::RwLock::new(Vec::new())),
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

    pub fn send_event(&self, event: PlayerEvent) {
        let _ = self.event_tx.send(event);
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
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
                metrics::QUEUE_LENGTH.set(0);
                sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        {
            let q = handle.queue.read().await;
            metrics::QUEUE_LENGTH.set(q.len() as i64);
        }

        info!("Now playing: {} ({})", track.title, track.path.display());

        *handle.now_playing.write().unwrap() = Some(track.clone());
        let started_at = unix_now();
        handle.send_event(PlayerEvent::TrackStarted(track.clone()));

        let track_token = CancellationToken::new();
        *handle.skip_token.write().await = track_token.clone();

        let was_skipped = stream_file(&track.path, &input_tx, &track_token, &shutdown).await;
        let duration_secs = unix_now() - started_at;

        if was_skipped {
            metrics::TRACKS_SKIPPED.inc();
            handle.send_event(PlayerEvent::TrackSkipped {
                track: track.clone(),
                duration_secs,
            });
            info!("Skipped: {}", track.title);
        } else {
            handle.send_event(PlayerEvent::TrackFinished {
                track: track.clone(),
                duration_secs,
            });
        }

        metrics::TRACKS_PLAYED.inc();

        // Record history
        {
            let mut history = handle.history.write().unwrap();
            history.push(HistoryEntry {
                track: track.clone(),
                started_at,
                duration_secs,
                skipped: was_skipped,
            });
            // Keep last 1000 entries
            let len = history.len();
            if len > 1000 {
                history.drain(..len - 1000);
            }
        }

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
                    Ok(0) => return false,
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
