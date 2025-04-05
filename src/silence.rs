// src/silence.rs
//
// Embedded silence Ogg Vorbis data

use std::io::Cursor;
use ogg::PacketReader;

use crate::SilenceData;

/// Raw bytes of a pre-encoded Ogg Vorbis file containing silence
/// This can be generated with:
/// ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 3 -c:a libvorbis -q:a 2 silence.ogg
static SILENCE_OGG_DATA: &[u8] = include_bytes!("../resources/silence.ogg");

/// Load the embedded silence data
pub fn load_embedded_silence() -> Result<SilenceData, Box<dyn std::error::Error>> {
    // Create a cursor over the embedded data
    let cursor = Cursor::new(SILENCE_OGG_DATA);
    let mut reader = PacketReader::new(cursor);

    let mut packets = Vec::new();
    let mut total_size = 0;
    let mut last_granule = 0i64;

    while let Some(packet) = reader.read_packet()? {
        // Calculate granule position increment
        let granule_increment = if packets.is_empty() {
            packet.absgp_page()
        } else {
            packet.absgp_page() - last_granule
        };

        last_granule = packet.absgp_page();

        // Store packet data and granule increment
        let data = packet.data.to_vec();
        total_size += data.len();
        packets.push((data, granule_increment));
    }

    Ok(SilenceData {
        packets,
        total_size,
    })
}
