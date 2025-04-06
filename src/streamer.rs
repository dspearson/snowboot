// src/streamer.rs
//
// Module for streaming Ogg data from pipe to Icecast

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, Context};
use log::{debug, error, info, trace};
use ogg::PacketReader;
use tokio::time::sleep;

use crate::icecast::{IcecastClient, IcecastConfig};
use crate::silence::SilenceData;

#[derive(Debug)]
pub enum StreamerError {
    IoError(io::Error),
    OggError(ogg::Error),
    IcecastError(IcecastError),
    InputFileMissing(String),
    StreamInterrupted,
}

impl From<io::Error> for StreamerError {
    fn from(err: io::Error) -> Self {
        StreamerError::IoError(err)
    }
}

impl From<ogg::Error> for StreamerError {
    fn from(err: ogg::Error) -> Self {
        StreamerError::OggError(err)
    }
}

impl From<IcecastError> for StreamerError {
    fn from(err: IcecastError) -> Self {
        StreamerError::IcecastError(err)
    }
}

pub struct OggStreamer {
    input_path: String,
    icecast_client: IcecastClient,
    running: Arc<AtomicBool>,
    max_silence_duration: Duration,
    silence_data: Option<SilenceData>,
    keep_alive: bool,
}

impl OggStreamer {
    pub fn new(
        input_path: String,
        icecast_config: IcecastConfig,
        running: Arc<AtomicBool>,
        max_silence_duration: Duration,
        silence_data: Option<SilenceData>,
        keep_alive: bool,
    ) -> Self {
        Self {
            input_path,
            icecast_client: IcecastClient::new(icecast_config),
            running,
            max_silence_duration,
            silence_data,
            keep_alive,
        }
    }

    pub async fn run(&mut self) -> Result<(), StreamerError> {
        // Connect to Icecast server
        self.icecast_client.connect().await?;

        let mut silence_mode = false;
        let mut silence_start_time = Instant::now();

        // Main streaming loop
        while self.running.load(Ordering::SeqCst) && self.icecast_client.is_running() {
            if silence_mode {
                // We're in silence mode (input error occurred)
                if !self.keep_alive {
                    error!("Input error occurred and keep-alive is disabled. Stopping stream.");
                    break;
                }

                // Check if we've exceeded max silence duration
                if self.max_silence_duration.as_secs() > 0 &&
                   silence_start_time.elapsed() > self.max_silence_duration {
                    error!("Maximum silence duration of {} seconds exceeded. Stopping stream.",
                          self.max_silence_duration.as_secs());
                    break;
                }

                // Send silence packets
                if let Some(silence_data) = &self.silence_data {
                    for (packet_data, _) in &silence_data.packets {
                        if !self.running.load(Ordering::SeqCst) {
                            break;
                        }

                        match self.icecast_client.send_data(packet_data).await {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Failed to send silence packet: {}", e);
                                return Err(StreamerError::IcecastError(e));
                            }
                        }
                    }
                    debug!("Sent silence packets");
                    sleep(Duration::from_millis(500)).await;
                } else {
                    error!("No silence data available. Stopping stream.");
                    break;
                }

                // Try to reopen the input
                match self.open_input() {
                    Ok(input) => {
                        info!("Successfully reopened input pipe");
                        silence_mode = false;
                        self.stream_from_input(input).await?;
                    }
                    Err(e) => {
                        debug!("Failed to reopen input: {}", e);
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            } else {
                // Normal streaming mode
                match self.open_input() {
                    Ok(input) => {
                        match self.stream_from_input(input).await {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Streaming error: {:?}", e);
                                silence_mode = true;
                                silence_start_time = Instant::now();
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to open input: {}", e);
                        silence_mode = true;
                        silence_start_time = Instant::now();
                    }
                }
            }
        }

        // Disconnect from Icecast
        self.icecast_client.disconnect().await?;
        Ok(())
    }

    fn open_input(&self) -> Result<BufReader<File>, io::Error> {
        let path = Path::new(&self.input_path);
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Input file not found: {}", self.input_path),
            ));
        }

        let file = File::open(path)?;
        Ok(BufReader::new(file))
    }

    async fn stream_from_input(&mut self, mut reader: BufReader<File>) -> Result<(), StreamerError> {
        // Create an Ogg packet reader
        let mut packet_reader = PacketReader::new(reader);

        info!("Started streaming from {}", self.input_path);

        // Read and send Ogg packets
        while self.running.load(Ordering::SeqCst) && self.icecast_client.is_running() {
            match packet_reader.read_packet() {
                Ok(Some(packet)) => {
                    trace!("Read packet of size {} bytes", packet.data.len());
                    self.icecast_client.send_data(&packet.data).await?;
                },
                Ok(None) => {
                    debug!("End of input reached");
                    break;
                },
                Err(e) => {
                    error!("Error reading Ogg packet: {}", e);
                    return Err(StreamerError::OggError(e));
                }
            }
        }

        debug!("Stopped streaming from input");
        Ok(())
    }
}
