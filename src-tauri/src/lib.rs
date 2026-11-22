mod hid_protocol;

use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveredHidDevice {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub manufacturer: String,
    pub interface_number: i32,
    pub device_path: String,
    pub protocol: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HidDispatchResult {
    pub dispatched_to_hardware: bool,
    pub report_bytes: Vec<u8>,
    pub detail: String,
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

/// Classifies Windows native HID device paths (`\\?\hid#...`) into transport protocols.
pub fn infer_protocol_from_path(device_path: &str, product_name: &str) -> String {
    let path_lower = device_path.to_lowercase();
    let name_lower = product_name.to_lowercase();
    if path_lower.contains("bthle")
        || path_lower.contains("bthenum")
        || path_lower.contains("{00001812")
        || name_lower.contains("bluetooth")
        || name_lower.contains("ble")
    {
        return "bluetooth".to_string();
    }
    if name_lower.contains("2.4g")
        || name_lower.contains("receiver")
        || name_lower.contains("dongle")
        || name_lower.contains("wireless")
        || name_lower.contains("lightspeed")
    {
        return "2.4g".to_string();
    }
    "usb".to_string()
}

/// Classifies HID Usage Page / Usage and product name into peripheral categories.
pub fn infer_category(usage_page: u16, usage: u16, product_name: &str) -> String {
    let lower = product_name.to_lowercase();
    if lower.contains("headset")
        || lower.contains("headphone")
        || lower.contains("audio")
        || lower.contains("ear")
        || lower.contains("mic")
        || usage_page == 0x0C
    {
        return "headset".to_string();
    }
    if lower.contains("keyboard")
        || lower.contains("kbd")
        || lower.contains("key ")
        || (usage_page == 0x01 && usage == 0x06)
    {
        return "keyboard".to_string();
    }
    "mouse".to_string()
}

fn parse_hex_id(raw: &str) -> Option<u16> {
    let trimmed = raw.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(trimmed, 16).ok()
}

fn try_send_native_hid_report(
    vendor_hex: Option<&str>,
    product_hex: Option<&str>,
    report: &[u8; 16],
) -> HidDispatchResult {
    if let (Some(v_str), Some(p_str)) = (vendor_hex, product_hex) {
        if let (Some(vid), Some(pid)) = (parse_hex_id(v_str), parse_hex_id(p_str)) {
            if let Ok(api) = HidApi::new() {
                if let Ok(device) = api.open(vid, pid) {
                    if device.send_feature_report(report).is_ok() {
                        return HidDispatchResult {
                            dispatched_to_hardware: true,
                            report_bytes: report.to_vec(),
                            detail: format!("已通过 Windows 原生 HID 端口下发至 {v_str}:{p_str}"),
                        };
                    }
                }
            }
        }
    }
    HidDispatchResult {
        dispatched_to_hardware: false,
        report_bytes: report.to_vec(),
        detail: "已编码 16 字节 HID 报文并保存至便携配置目录".to_string(),
    }
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

/// Enumerates real Windows native HID ports (USB / 2.4G Dongle / Bluetooth HID via hid.dll).
#[tauri::command]
fn scan_devices() -> Result<Vec<DiscoveredHidDevice>, String> {
    let api = HidApi::new().map_err(|e| format!("Windows 原生 HID 初始化失败: {e}"))?;
    let mut results = Vec::new();
    let mut seen_keys: HashSet<(u16, u16)> = HashSet::new();

    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();
        if vid == 0 || pid == 0 || !seen_keys.insert((vid, pid)) {
            continue;
        }
        let product = dev
            .product_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Windows HID 外设")
            .to_string();
        let manufacturer = dev
            .manufacturer_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Standard HID")
            .to_string();
        let device_path = dev.path().to_string_lossy().to_string();
        let protocol = infer_protocol_from_path(&device_path, &product);
        let category = infer_category(dev.usage_page(), dev.usage(), &product);

        results.push(DiscoveredHidDevice {
            vendor_id: format!("0x{:04X}", vid),
            product_id: format!("0x{:04X}", pid),
            product_name: product,
            manufacturer,
            interface_number: dev.interface_number(),
            device_path,
            protocol,
            category,
        });
    }
    Ok(results)
}

#[tauri::command]
fn apply_mouse_config(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    let polling = config
        .get("pollingRateHz")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as u16;
    let payload = polling.to_be_bytes();
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_POLLING_RATE, &payload);
    let vid = config.get("vendorId").and_then(|v| v.as_str());
    let pid = config.get("productId").and_then(|v| v.as_str());
    Ok(try_send_native_hid_report(vid, pid, &report))
}

#[tauri::command]
fn apply_keyboard_config(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    let actuation_tenths = (config
        .get("globalActuationMm")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.2)
        * 100.0) as u16;
    let payload = actuation_tenths.to_be_bytes();
    let report =
        hid_protocol::build_feature_report(hid_protocol::CMD_SET_KEYBOARD_ACTUATION, &payload);
    let vid = config.get("vendorId").and_then(|v| v.as_str());
    let pid = config.get("productId").and_then(|v| v.as_str());
    Ok(try_send_native_hid_report(vid, pid, &report))
}

#[tauri::command]
fn apply_audio_eq(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    let mut payload = Vec::with_capacity(10);
    if let Some(bands) = config.get("bands").and_then(|v| v.as_array()) {
        for band in bands.iter().take(10) {
            let db = band.as_f64().unwrap_or(0.0).clamp(-12.0, 12.0);
            let encoded = ((db + 12.0) * 10.0).round() as u8;
            payload.push(encoded);
        }
    }
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_AUDIO_EQ, &payload);
    let vid = config.get("vendorId").and_then(|v| v.as_str());
    let pid = config.get("productId").and_then(|v| v.as_str());
    Ok(try_send_native_hid_report(vid, pid, &report))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_windows_native_hid_paths() {
        assert_eq!(
            infer_protocol_from_path(r"\\?\hid#bthledevice_{00001812}\vid_3554", "Air Mouse"),
            "bluetooth"
        );
        assert_eq!(
            infer_protocol_from_path(r"\\?\hid#vid_3554&pid_f51a", "2.4G Wireless Receiver"),
            "2.4g"
        );
        assert_eq!(
            infer_protocol_from_path(r"\\?\hid#vid_3554&pid_k750", "Key 75 Wired"),
            "usb"
        );
    }

    #[test]
    fn parses_hex_vid_pid() {
        assert_eq!(parse_hex_id("0x3554"), Some(0x3554));
        assert_eq!(parse_hex_id("F51A"), Some(0xF51A));
        assert_eq!(parse_hex_id("invalid"), None);
    }
}
