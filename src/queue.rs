use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

static NEXT_TRACK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
}

impl Track {
    pub fn new(path: PathBuf, title: String) -> Self {
        Self {
            id: NEXT_TRACK_ID.fetch_add(1, Ordering::Relaxed),
            path,
            title,
        }
    }
}

#[derive(Debug, Default)]
pub struct Queue {
    tracks: VecDeque<Track>,
}

impl Queue {
    pub fn push_back(&mut self, track: Track) {
        self.tracks.push_back(track);
    }

    pub fn push_front(&mut self, track: Track) {
        self.tracks.push_front(track);
    }

    pub fn pop_front(&mut self) -> Option<Track> {
        self.tracks.pop_front()
    }

    pub fn list(&self) -> Vec<Track> {
        self.tracks.iter().cloned().collect()
    }

    pub fn remove(&mut self, id: u64) -> Option<Track> {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            self.tracks.remove(pos)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn move_track(&mut self, id: u64, position: usize) -> bool {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            let track = self.tracks.remove(pos).unwrap();
            let insert_at = position.min(self.tracks.len());
            self.tracks.insert(insert_at, track);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

pub type SharedQueue = Arc<RwLock<Queue>>;
