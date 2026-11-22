mod hid_protocol;

use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveredHidDevice {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub manufacturer: String,
    pub interface_number: i32,
}

fn portable_data_dir() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "Unable to resolve executable directory".to_string())?;
    let data_dir = exe_dir.join("LumenHID.data");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir)
}

#[tauri::command]
fn get_portable_config_path() -> Result<String, String> {
    let path = portable_data_dir()?.join("profile.json");
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn save_portable_profile(payload: String) -> Result<String, String> {
    let path = portable_data_dir()?.join("profile.json");
    fs::write(&path, payload).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn load_portable_profile() -> Result<Option<String>, String> {
    let path = portable_data_dir()?.join("profile.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok(Some(content))
}

#[tauri::command]
fn scan_devices() -> Result<Vec<DiscoveredHidDevice>, String> {
    let api = HidApi::new().map_err(|e| format!("HID init failed: {e}"))?;
    let mut results = Vec::new();
    for dev in api.device_list() {
        let product = dev
            .product_string()
            .unwrap_or("Generic HID Device")
            .to_string();
        let manufacturer = dev
            .manufacturer_string()
            .unwrap_or("Unknown Vendor")
            .to_string();
        results.push(DiscoveredHidDevice {
            vendor_id: format!("0x{:04X}", dev.vendor_id()),
            product_id: format!("0x{:04X}", dev.product_id()),
            product_name: product,
            manufacturer,
            interface_number: dev.interface_number(),
        });
    }
    Ok(results)
}

#[tauri::command]
fn apply_mouse_config(config: serde_json::Value) -> Result<Vec<u8>, String> {
    let polling = config
        .get("pollingRateHz")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as u16;
    let payload = polling.to_be_bytes();
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_POLLING_RATE, &payload);
    Ok(report.to_vec())
}

#[tauri::command]
fn apply_keyboard_config(config: serde_json::Value) -> Result<Vec<u8>, String> {
    let actuation_tenths = (config
        .get("globalActuationMm")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.2)
        * 100.0) as u16;
    let payload = actuation_tenths.to_be_bytes();
    let report =
        hid_protocol::build_feature_report(hid_protocol::CMD_SET_KEYBOARD_ACTUATION, &payload);
    Ok(report.to_vec())
}

#[tauri::command]
fn apply_audio_eq(config: serde_json::Value) -> Result<Vec<u8>, String> {
    let mut payload = Vec::with_capacity(10);
    if let Some(bands) = config.get("bands").and_then(|v| v.as_array()) {
        for band in bands.iter().take(10) {
            let db = band.as_f64().unwrap_or(0.0).clamp(-12.0, 12.0);
            let encoded = ((db + 12.0) * 10.0).round() as u8;
            payload.push(encoded);
        }
    }
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_AUDIO_EQ, &payload);
    Ok(report.to_vec())
}

#[tauri::command]
fn execute_macro(macro_def: serde_json::Value) -> Result<usize, String> {
    let step_count = macro_def
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|s| s.len())
        .unwrap_or(0);
    Ok(step_count)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_portable_config_path,
            save_portable_profile,
            load_portable_profile,
            scan_devices,
            apply_mouse_config,
            apply_keyboard_config,
            apply_audio_eq,
            execute_macro,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LumenHID Tauri application");
}
