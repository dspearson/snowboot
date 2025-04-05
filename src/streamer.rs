// src/streamer.rs
//
// Module for streaming Ogg data from pipe to Icecast

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use log::{debug, error, info, trace};
use ogg::PacketReader;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::icecast::{IcecastClient, IcecastConfig};
use crate::silence::SilenceData;

/// Commands for the packet processor worker
pub enum PacketCommand {
    Process(Vec<u8>),
    Stop,
}

/// Status messages from the packet processor
pub enum PacketStatus {
    Processed { size: usize },
    Error(String),
    Stopped,
}

/// Commands for the input reader worker
pub enum ReaderCommand {
    Stop,
    CheckInputFile,
}

/// Status messages from the input reader
pub enum ReaderStatus {
    FileOpened,
    FileError(String),
    PacketRead { data: Vec<u8> },
    EndOfFile,
    Error(String),
    Stopped,
}

/// Operational modes for the streamer
#[derive(Clone, Debug)]
enum StreamerMode {
    /// Normal streaming mode, reading from the input file
    Normal,
    /// Silence mode, used when input file has errors
    Silence { since: Instant },
}

/// Main struct for streaming Ogg data
pub struct OggStreamer {
    input_path: String,
    icecast_config: IcecastConfig,
    running: Arc<AtomicBool>,
    max_silence_duration: Duration,
    silence_data: Option<SilenceData>,
    keep_alive: bool,

    // Workers and channels
    reader_cmd_tx: Option<Sender<ReaderCommand>>,
    reader_status_rx: Option<Receiver<ReaderStatus>>,
    reader_handle: Option<JoinHandle<()>>,

    processor_cmd_tx: Option<Sender<PacketCommand>>,
    processor_status_rx: Option<Receiver<PacketStatus>>,
    processor_handle: Option<JoinHandle<()>>,

    // Current mode
    mode: Arc<AtomicBool>, // true = Normal, false = Silence

    // Icecast client
    icecast_client: IcecastClient,
}

impl OggStreamer {
    /// Create a new OggStreamer instance
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
            icecast_config: icecast_config.clone(),
            running,
            max_silence_duration,
            silence_data,
            keep_alive,
            reader_cmd_tx: None,
            reader_status_rx: None,
            reader_handle: None,
            processor_cmd_tx: None,
            processor_status_rx: None,
            processor_handle: None,
            mode: Arc::new(AtomicBool::new(true)), // Start in normal mode
            icecast_client: IcecastClient::new(icecast_config),
        }
    }

    /// Start the streaming process
    pub async fn run(&mut self) -> Result<()> {
        // Connect to Icecast server
        self.icecast_client.connect().await?;

        // Start the packet processor worker
        self.start_packet_processor().await?;

        // Start the file reader worker
        self.start_file_reader().await?;

        // Main control loop - coordinate the workers
        self.control_loop().await?;

        // Shutdown workers
        self.shutdown_workers().await;

        // Disconnect from Icecast
        self.icecast_client.disconnect().await?;

        Ok(())
    }

    /// Start the packet processor worker
    async fn start_packet_processor(&mut self) -> Result<()> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<PacketCommand>(100);
        let (status_tx, status_rx) = mpsc::channel::<PacketStatus>(100);

        self.processor_cmd_tx = Some(cmd_tx);
        self.processor_status_rx = Some(status_rx);

        let mut icecast_client = self.icecast_client.clone();
        let running = self.running.clone();

        self.processor_handle = Some(tokio::spawn(async move {
            if let Err(e) = Self::packet_processor_worker(
                cmd_rx,
                status_tx,
                &mut icecast_client,
                running
            ).await {
                error!("Packet processor error: {}", e);
            }
        }));

        Ok(())
    }

    /// Start the file reader worker
    async fn start_file_reader(&mut self) -> Result<()> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ReaderCommand>(32);
        let (status_tx, status_rx) = mpsc::channel::<ReaderStatus>(100);

        self.reader_cmd_tx = Some(cmd_tx);
        self.reader_status_rx = Some(status_rx);

        let input_path = self.input_path.clone();
        let running = self.running.clone();

        self.reader_handle = Some(tokio::spawn(async move {
            if let Err(e) = Self::file_reader_worker(
                input_path,
                running,
                cmd_rx,
                status_tx,
            ).await {
                error!("File reader error: {}", e);
            }
        }));

        Ok(())
    }

    /// Main control loop that coordinates workers
    async fn control_loop(&mut self) -> Result<()> {
        let mut check_interval = tokio::time::interval(Duration::from_secs(5));
        let mut silence_start = Instant::now();

        while self.running.load(Ordering::SeqCst) && self.icecast_client.is_connected() {
            tokio::select! {
                // Check for status updates from the reader
                reader_status = self.reader_status_rx.as_mut().unwrap().recv(), if self.reader_status_rx.is_some() => {
                    if let Some(status) = reader_status {
                        self.handle_reader_status(status, &mut silence_start).await?;
                    } else {
                        // Channel closed, reader is done
                        break;
                    }
                }

                // Check for status updates from the processor
                processor_status = self.processor_status_rx.as_mut().unwrap().recv(), if self.processor_status_rx.is_some() => {
                    if let Some(status) = processor_status {
                        self.handle_processor_status(status).await?;
                    } else {
                        // Channel closed, processor is done
                        break;
                    }
                }

                // Periodically check the input file
                _ = check_interval.tick() => {
                    if let Some(tx) = &self.reader_cmd_tx {
                        let _ = tx.send(ReaderCommand::CheckInputFile).await;
                    }

                    // If in silence mode, check if we should try to reopen the input
                    if !self.mode.load(Ordering::SeqCst) {
                        // In silence mode
                        if silence_start.elapsed() > Duration::from_secs(1) {
                            // Try to reopen the input every second
                            if let Some(tx) = &self.reader_cmd_tx {
                                debug!("Trying to reopen input file");
                                let _ = tx.send(ReaderCommand::CheckInputFile).await;
                            }

                            // Check if we've exceeded max silence duration
                            if self.max_silence_duration.as_secs() > 0 &&
                               silence_start.elapsed() > self.max_silence_duration {
                                error!("Maximum silence duration of {} seconds exceeded. Stopping stream.",
                                       self.max_silence_duration.as_secs());
                                break;
                            }

                            // Send silence packets
                            if let Some(silence_data) = &self.silence_data {
                                self.stream_silence_packets(silence_data).await?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Stream silence packets during input errors
    async fn stream_silence_packets(&self, silence_data: &SilenceData) -> Result<()> {
        for (packet_data, _) in &silence_data.packets {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            if let Some(tx) = &self.processor_cmd_tx {
                tx.send(PacketCommand::Process(packet_data.clone())).await
                    .map_err(|_| anyhow!("Failed to send silence packet to processor"))?;
            }
        }

        debug!("Sent silence packets");
        Ok(())
    }

    /// Handle status updates from the file reader
    async fn handle_reader_status(&mut self, status: ReaderStatus, silence_start: &mut Instant) -> Result<()> {
        match status {
            ReaderStatus::FileOpened => {
                info!("Successfully opened input file");
                // Switch to normal mode
                self.mode.store(true, Ordering::SeqCst);
            },
            ReaderStatus::FileError(msg) => {
                error!("Input file error: {}", msg);
                if !self.keep_alive {
                    return Err(anyhow!("Input file error and keep-alive is disabled"));
                }

                // Switch to silence mode
                self.mode.store(false, Ordering::SeqCst);
                *silence_start = Instant::now();
            },
            ReaderStatus::PacketRead { data } => {
                trace!("Read packet of {} bytes", data.len());
                // Forward the packet to the processor
                if let Some(tx) = &self.processor_cmd_tx {
                    tx.send(PacketCommand::Process(data)).await
                        .map_err(|_| anyhow!("Failed to send packet to processor"))?;
                }
            },
            ReaderStatus::EndOfFile => {
                debug!("End of input file reached");
                // Try to reopen the file
                if let Some(tx) = &self.reader_cmd_tx {
                    let _ = tx.send(ReaderCommand::CheckInputFile).await;
                }
            },
            ReaderStatus::Error(msg) => {
                error!("Reader error: {}", msg);
                if !self.keep_alive {
                    return Err(anyhow!("Reader error and keep-alive is disabled"));
                }

                // Switch to silence mode
                self.mode.store(false, Ordering::SeqCst);
                *silence_start = Instant::now();
            },
            ReaderStatus::Stopped => {
                debug!("Reader worker stopped");
            },
        }

        Ok(())
    }

    /// Handle status updates from the packet processor
    async fn handle_processor_status(&mut self, status: PacketStatus) -> Result<()> {
        match status {
            PacketStatus::Processed { size } => {
                trace!("Processed packet of {} bytes", size);
            },
            PacketStatus::Error(msg) => {
                error!("Packet processor error: {}", msg);
                if !self.keep_alive {
                    return Err(anyhow!("Packet processor error and keep-alive is disabled"));
                }
            },
            PacketStatus::Stopped => {
                debug!("Packet processor stopped");
            },
        }

        Ok(())
    }

    /// Shutdown all workers
    async fn shutdown_workers(&mut self) {
        // Stop the packet processor
        if let Some(tx) = &self.processor_cmd_tx {
            let _ = tx.send(PacketCommand::Stop).await;
        }

        // Stop the file reader
        if let Some(tx) = &self.reader_cmd_tx {
            let _ = tx.send(ReaderCommand::Stop).await;
        }

        // Wait for workers to finish
        if let Some(handle) = self.reader_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }

        if let Some(handle) = self.processor_handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }

    /// Worker function that processes Ogg packets and sends them to Icecast
    async fn packet_processor_worker(
        mut cmd_rx: Receiver<PacketCommand>,
        status_tx: Sender<PacketStatus>,
        icecast_client: &mut IcecastClient,
        running: Arc<AtomicBool>,
    ) -> Result<()> {
        info!("Packet processor started");

        while running.load(Ordering::SeqCst) {
            match cmd_rx.recv().await {
                Some(PacketCommand::Process(data)) => {
                    // Process and send the packet to Icecast
                    match icecast_client.send_data(&data).await {
                        Ok(_) => {
                            let _ = status_tx.send(PacketStatus::Processed { size: data.len() }).await;
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to send packet: {}", e);
                            error!("{}", error_msg);
                            let _ = status_tx.send(PacketStatus::Error(error_msg)).await;
                        }
                    }
                },
                Some(PacketCommand::Stop) => {
                    debug!("Received stop command for packet processor");
                    break;
                },
                None => {
                    debug!("Command channel closed for packet processor");
                    break;
                }
            }
        }

        let _ = status_tx.send(PacketStatus::Stopped).await;
        debug!("Packet processor stopped");

        Ok(())
    }

    /// Worker function that reads Ogg packets from the input file
    async fn file_reader_worker(
        input_path: String,
        running: Arc<AtomicBool>,
        mut cmd_rx: Receiver<ReaderCommand>,
        status_tx: Sender<ReaderStatus>,
    ) -> Result<()> {
        info!("File reader started for {}", input_path);

        // Try to open the input file initially
        let mut input_reader = match Self::open_input_file(&input_path) {
            Ok(reader) => {
                let _ = status_tx.send(ReaderStatus::FileOpened).await;
                Some(reader)
            },
            Err(e) => {
                let error_msg = format!("Failed to open input file: {}", e);
                let _ = status_tx.send(ReaderStatus::FileError(error_msg)).await;
                None
            }
        };

        while running.load(Ordering::SeqCst) {
            tokio::select! {
                // Check for commands
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(ReaderCommand::Stop) => {
                            debug!("Received stop command for file reader");
                            break;
                        },
                        Some(ReaderCommand::CheckInputFile) => {
                            if input_reader.is_none() {
                                // Try to open the input file
                                match Self::open_input_file(&input_path) {
                                    Ok(reader) => {
                                        input_reader = Some(reader);
                                        let _ = status_tx.send(ReaderStatus::FileOpened).await;
                                    },
                                    Err(e) => {
                                        let error_msg = format!("Failed to open input file: {}", e);
                                        let _ = status_tx.send(ReaderStatus::FileError(error_msg)).await;
                                    }
                                }
                            }
                        },
                        None => {
                            debug!("Command channel closed for file reader");
                            break;
                        }
                    }
                },

                // Process the input file if available
                _ = async {}, if input_reader.is_some() => {
                    // Use a block to limit the scope of the borrow
                    {
                        let reader = input_reader.as_mut().unwrap();

                        // Process a packet, but do it in a way that doesn't move the reader
                        match Self::read_next_packet(reader).await {
                            Ok(Some(packet_data)) => {
                                let _ = status_tx.send(ReaderStatus::PacketRead { data: packet_data }).await;
                            },
                            Ok(None) => {
                                let _ = status_tx.send(ReaderStatus::EndOfFile).await;
                                // Reset the reader to trigger reopening
                                input_reader = None;
                            },
                            Err(e) => {
                                let error_msg = format!("Error reading packet: {}", e);
                                let _ = status_tx.send(ReaderStatus::Error(error_msg)).await;
                                // Reset the reader to trigger reopening
                                input_reader = None;
                            }
                        }
                    }

                    // Small delay to avoid tight loops
                    sleep(Duration::from_millis(1)).await;
                }
            }
        }

        let _ = status_tx.send(ReaderStatus::Stopped).await;
        debug!("File reader stopped");

        Ok(())
    }

    /// Open the input file
    fn open_input_file(path: &str) -> io::Result<BufReader<File>> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Input file not found: {}", path.display()),
            ));
        }

        let file = File::open(path)?;
        Ok(BufReader::new(file))
    }

    /// Read the next Ogg packet from the reader
    async fn read_next_packet(reader: &mut BufReader<File>) -> Result<Option<Vec<u8>>> {
        // We need to handle the packet reading in a way that doesn't transfer ownership
        // of the reader across an await point

        // Create a new packet reader for this operation
        let mut packet_reader = PacketReader::new(reader);

        // Read a single packet
        match packet_reader.read_packet() {
            Ok(Some(packet)) => Ok(Some(packet.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(anyhow!("Error reading Ogg packet: {}", e)),
        }
    }
}
