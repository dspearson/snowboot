// src/silence.rs
//
// Module for handling silence data for keep-alive functionality

use anyhow;
use log::debug;
use std::io;

// Include pre-generated silence data from the resources directory
// This embeds the file directly into the binary at compile time
const EMBEDDED_SILENCE_DATA: &[u8] = include_bytes!("../resources/silence.ogg");

/// Preloaded silence data
pub struct SilenceData {
    pub packets: Vec<(Vec<u8>, u64)>, // (packet_data, granule_increment)
    pub total_size: usize,
}

/// Load embedded silence packets for keep-alive functionality
///
/// This function reads the silence.ogg file that was embedded at compile time
/// from the resources directory and parses it into Ogg packets that can be
/// sent during stream interruptions.
pub fn load_embedded_silence() -> anyhow::Result<SilenceData> {
    debug!(
        "Loading embedded silence data ({} bytes)",
        EMBEDDED_SILENCE_DATA.len()
    );

    // Create a cursor to read from the embedded data
    let mut reader = io::Cursor::new(EMBEDDED_SILENCE_DATA);
    let mut packet_reader = ogg::PacketReader::new(&mut reader);

    let mut silence_packets = Vec::new();
    let mut total_size = 0;

    // Extract all packets from the embedded silence data
    while let Some(packet) = packet_reader
        .read_packet()
        .map_err(|e| anyhow::anyhow!("Failed to read Ogg packet: {}", e))?
    {
        let packet_size = packet.data.len();
        total_size += packet_size;

        // Store the packet data and a default granule increment (usually 48 for Vorbis at 48kHz)
        // The granule increment is approximately the number of samples in the packet
        silence_packets.push((packet.data, 48));
    }

    if silence_packets.is_empty() {
        return Err(anyhow::anyhow!("No silence packets found in embedded data"));
    }

    debug!(
        "Successfully loaded {} silence packets totaling {} bytes",
        silence_packets.len(),
        total_size
    );

    Ok(SilenceData {
        packets: silence_packets,
        total_size,
    })
}

/// Generate a simple silence Ogg page
/// This is a fallback in case the embedded silence data is not available
pub fn generate_basic_silence() -> anyhow::Result<SilenceData> {
    // This is a simplistic approach - in a real implementation, you'd want to:
    // 1. Create proper Vorbis headers (identification, comment, and setup)
    // 2. Create properly formatted Vorbis audio packets with silence

    // For now, we'll just create a very basic placeholder
    let mut packets = Vec::new();

    // Simple silent packet (this is not actually valid Vorbis data)
    let silent_packet = vec![0u8; 64];
    packets.push((silent_packet.clone(), 48));

    Ok(SilenceData {
        packets,
        total_size: 64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_embedded_silence() {
        let result = load_embedded_silence();
        assert!(result.is_ok(), "Should load silence data without errors");

        if let Ok(data) = result {
            assert!(!data.packets.is_empty(), "Should have at least one packet");
            assert!(
                data.total_size > 0,
                "Total size should be greater than zero"
            );
        }
    }
}
