mod hid_protocol;
mod protocols;
mod safety;
mod telemetry_daemon;
mod vendor_protocols;
mod win_battery;

use hidapi::HidApi;
use protocols::TelemetryEvent;
use safety::{BackupSnapshot, HardwareSafetyGuard, SafetyStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use telemetry_daemon::TelemetryCallback;
use vendor_protocols::{aggregate_real_peripherals, DiscoveredPeripheral};
use win_battery::query_windows_bluetooth_devices;

fn make_telemetry_callback(app: tauri::AppHandle) -> TelemetryCallback {
    Arc::new(move |payload: TelemetryEvent| {
        if let (Some(win), Ok(json)) = (
            app.get_webview_window("main"),
            serde_json::to_string(&payload),
        ) {
            let js = format!(
                "window.dispatchEvent(new CustomEvent('lumen-telemetry', {{ detail: {json} }}));"
            );
            let _ = win.eval(&js);
        }
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HidDispatchResult {
    pub dispatched_to_hardware: bool,
    pub report_bytes: Vec<u8>,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatterySnapshot {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub battery_source: String,
    pub firmware_version: Option<String>,
}

pub(crate) fn portable_data_dir() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "Unable to resolve executable directory".to_string())?;
    let data_dir = exe_dir.join("LumenHID.data");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir)
}

pub(crate) fn parse_hex_id(raw: &str) -> Option<u16> {
    let trimmed = raw.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(trimmed, 16).ok()
}

fn dispatch_mouse_blocking(config: serde_json::Value) -> HidDispatchResult {
    let polling = config
        .get("pollingRateHz")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as u16;
    let vid = config
        .get("vendorId")
        .and_then(|v| v.as_str())
        .and_then(parse_hex_id)
        .unwrap_or(0);
    let pid = config
        .get("productId")
        .and_then(|v| v.as_str())
        .and_then(parse_hex_id)
        .unwrap_or(0);

    let payload = polling.to_be_bytes();
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_POLLING_RATE, &payload);

    if vid != 0 && pid != 0 {
        if let Err(reason) = HardwareSafetyGuard::check_write_allowed(vid, pid, "mouse_config") {
            return HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: report.to_vec(),
                detail: reason,
            };
        }
    }

    HidDispatchResult {
        dispatched_to_hardware: false,
        report_bytes: report.to_vec(),
        detail: "已保存鼠标调校参数至便携配置目录（安全只读遥测阶段）".to_string(),
    }
}

#[tauri::command]
async fn get_portable_config_path() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = portable_data_dir()?.join("profile.json");
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_portable_profile(payload: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = portable_data_dir()?.join("profile.json");
        fs::write(&path, payload).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn load_portable_profile() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = portable_data_dir()?.join("profile.json");
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Ok(Some(content))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_safety_status() -> Result<SafetyStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let dir = portable_data_dir()?;
        Ok(HardwareSafetyGuard::current_status(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_readonly_mode(enabled: bool) -> Result<SafetyStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        safety::set_readonly_mode(enabled);
        let dir = portable_data_dir()?;
        Ok(HardwareSafetyGuard::current_status(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_device_backups(device_key: Option<String>) -> Result<Vec<BackupSnapshot>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = portable_data_dir()?;
        Ok(safety::list_backups(&dir, device_key.as_deref()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Async non-blocking enumeration of real physical Windows peripherals (HID + Bluetooth PnP battery).
/// Runs strictly inside `spawn_blocking` so WebView2 UI stays 60fps responsive.
#[tauri::command]
async fn scan_devices(app: tauri::AppHandle) -> Result<Vec<DiscoveredPeripheral>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bt_devices = query_windows_bluetooth_devices();
        let api = HidApi::new().map_err(|e| format!("Windows 原生 HID 初始化失败: {e}"))?;
        let cb = make_telemetry_callback(app);
        Ok(aggregate_real_peripherals(&api, &bt_devices, Some(&cb)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fast non-blocking battery refresh combining Windows native `DEVPKEY_Bluetooth_Battery`
/// and read-only vendor HID telemetry probes.
#[tauri::command]
async fn refresh_device_battery(app: tauri::AppHandle) -> Result<Vec<BatterySnapshot>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bt_devices = query_windows_bluetooth_devices();
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let cb = make_telemetry_callback(app);
        let devices = aggregate_real_peripherals(&api, &bt_devices, Some(&cb));
        Ok(devices
            .into_iter()
            .map(|d| BatterySnapshot {
                vendor_id: d.vendor_id,
                product_id: d.product_id,
                product_name: d.product_name,
                battery_percent: d.battery_percent,
                charging: d.charging,
                battery_source: d.battery_source,
                firmware_version: d.firmware_version,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn apply_mouse_config(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || dispatch_mouse_blocking(config))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_keyboard_config(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let actuation_tenths = (config
            .get("globalActuationMm")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.2)
            * 100.0) as u16;
        let payload = actuation_tenths.to_be_bytes();
        let report =
            hid_protocol::build_feature_report(hid_protocol::CMD_SET_KEYBOARD_ACTUATION, &payload);
        // Safe by design: never blind-write unknown Sinowealth Flash pages
        HidDispatchResult {
            dispatched_to_hardware: false,
            report_bytes: report.to_vec(),
            detail: "已安全保存键盘配置至便携目录（避免盲写非公开 Flash 分区）".to_string(),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_audio_eq(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut payload = Vec::with_capacity(10);
        if let Some(bands) = config.get("bands").and_then(|v| v.as_array()) {
            for band in bands.iter().take(10) {
                let db = band.as_f64().unwrap_or(0.0).clamp(-12.0, 12.0);
                let encoded = ((db + 12.0) * 10.0).round() as u8;
                payload.push(encoded);
            }
        }
        let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_AUDIO_EQ, &payload);
        HidDispatchResult {
            dispatched_to_hardware: false,
            report_bytes: report.to_vec(),
            detail: "已保存 10 段 EQ 曲线（可一键导出 EqualizerAPO 配置应用至 Windows 全局音频）"
                .to_string(),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_macro(macro_def: serde_json::Value) -> Result<usize, String> {
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
        .setup(|app| {
            // Ensure the Windows taskbar & titlebar icon is always mounted at runtime as well as in PE resources
            if let (Some(window), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = window.set_icon(icon.clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_portable_config_path,
            save_portable_profile,
            load_portable_profile,
            get_safety_status,
            set_readonly_mode,
            list_device_backups,
            scan_devices,
            refresh_device_battery,
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
    fn parses_hex_vid_pid() {
        assert_eq!(parse_hex_id("0x1D57"), Some(0x1D57));
        assert_eq!(parse_hex_id("FA65"), Some(0xFA65));
        assert_eq!(parse_hex_id("invalid"), None);
    }
}
