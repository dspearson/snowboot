
// src/util/silence.rs
//
// Utilities for handling silence data

use anyhow::Result;
use log::{debug, error, info, warn};

use crate::silence;

/// Load silence data for keep-alive functionality
pub async fn load_data(keep_alive: bool) -> Result<Option<silence::SilenceData>> {
    if !keep_alive {
        debug!("Keep-alive functionality is disabled");
        return Ok(None);
    }

    match silence::load_embedded_silence() {
        Ok(data) => {
            info!("Loaded {} silence packets ({} bytes)",
                  data.packets.len(),
                  data.total_size);
            Ok(Some(data))
        },
        Err(e) => {
            error!("Failed to load silence data: {}", e);
            warn!("Keep-alive functionality will be disabled");
            Ok(None)
        }
    }
}
