//! Modular vendor protocol drivers and unified real-time peripheral telemetry events.

pub mod hecate_g5max;
pub mod inphic_in10;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub battery_source: String,
    pub firmware_version: Option<String>,
    pub active_dpi_stage: Option<u8>,
    pub model_id: Option<u8>,
}
