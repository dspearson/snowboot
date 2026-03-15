use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
}

impl Track {
    pub fn from_file(path: PathBuf) -> Self {
        let comments = read_vorbis_comments(&path).unwrap_or_default();

        let title = comments.get("TITLE")
            .cloned()
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            });

        let artist = comments.get("ARTIST").cloned();

        Self {
            id: NEXT_TRACK_ID.fetch_add(1, Ordering::Relaxed),
            path,
            title,
            artist,
        }
    }

    pub fn metadata_comments(&self) -> Vec<(String, String)> {
        let mut comments = vec![
            ("TITLE".to_string(), self.title.clone()),
        ];
        if let Some(ref artist) = self.artist {
            comments.push(("ARTIST".to_string(), artist.clone()));
        }
        comments
    }
}

/// Parse vorbis comments from an Ogg Vorbis file.
///
/// Reads the second packet (comment header) which starts with \x03vorbis,
/// then contains a vendor string followed by key=value comment pairs.
fn read_vorbis_comments(path: &Path) -> Option<HashMap<String, String>> {
    use ogg::reading::PacketReader;
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).ok()?;
    let mut reader = PacketReader::new(BufReader::new(file));

    // Skip identification header (first packet)
    reader.read_packet().ok()??;

    // Read comment header (second packet)
    let comment_pkt = reader.read_packet().ok()??;
    let data = &comment_pkt.data;

    // Must start with \x03vorbis
    if data.len() < 7 || &data[0..7] != b"\x03vorbis" {
        return None;
    }

    let mut pos = 7;

    // Vendor string length (u32 LE)
    if pos + 4 > data.len() { return None; }
    let vendor_len = u32::from_le_bytes(data[pos..pos+4].try_into().ok()?) as usize;
    pos += 4 + vendor_len;

    // Number of comments (u32 LE)
    if pos + 4 > data.len() { return None; }
    let count = u32::from_le_bytes(data[pos..pos+4].try_into().ok()?) as usize;
    pos += 4;

    let mut comments = HashMap::new();
    for _ in 0..count {
        if pos + 4 > data.len() { break; }
        let len = u32::from_le_bytes(data[pos..pos+4].try_into().ok()?) as usize;
        pos += 4;
        if pos + len > data.len() { break; }

        if let Ok(s) = std::str::from_utf8(&data[pos..pos+len]) {
            if let Some((key, value)) = s.split_once('=') {
                comments.insert(key.to_uppercase(), value.to_string());
            }
        }
        pos += len;
    }

    Some(comments)
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
