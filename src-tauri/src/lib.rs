mod hid_protocol;
mod keyboard_engine;
mod protocols;
mod safety;
mod telemetry_daemon;
mod updater;
mod vendor_protocols;
mod win_battery;

use hidapi::HidApi;
use keyboard_engine::KeyboardEngineStatus;
use protocols::TelemetryEvent;
use safety::{BackupSnapshot, HardwareSafetyGuard, SafetyStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;
use telemetry_daemon::TelemetryCallback;
use vendor_protocols::{aggregate_real_peripherals, DiscoveredPeripheral};
use win_battery::query_windows_bluetooth_devices;

static ACTIVE_KEYBOARD_CFG: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();
static ACTIVE_MOUSE_CFG: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();
static ACTIVE_AUDIO_CFG: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();

fn active_kbd_cfg_lock() -> &'static Mutex<Option<serde_json::Value>> {
    ACTIVE_KEYBOARD_CFG.get_or_init(|| Mutex::new(None))
}

fn active_mouse_cfg_lock() -> &'static Mutex<Option<serde_json::Value>> {
    ACTIVE_MOUSE_CFG.get_or_init(|| Mutex::new(None))
}

fn active_audio_cfg_lock() -> &'static Mutex<Option<serde_json::Value>> {
    ACTIVE_AUDIO_CFG.get_or_init(|| Mutex::new(None))
}

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
    let control_path = config
        .get("vendorControlPath")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let payload = polling.to_be_bytes();
    let fallback_report =
        hid_protocol::build_feature_report(hid_protocol::CMD_SET_POLLING_RATE, &payload);

    let is_inphic = vid == protocols::inphic_in10::VID_INPHIC
        && pid == protocols::inphic_in10::PID_KP8K_DONGLE;
    let preview_bytes = if is_inphic {
        protocols::inphic_in10::build_dpi_frame_from_config(&config)
            .map(|f| f.to_vec())
            .unwrap_or_else(|_| fallback_report.to_vec())
    } else {
        fallback_report.to_vec()
    };

    if vid != 0 && pid != 0 {
        if let Err(reason) = HardwareSafetyGuard::check_write_allowed(vid, pid, "mouse_config") {
            return HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: preview_bytes,
                detail: reason,
            };
        }
    }

    if is_inphic {
        let Some(cpath) = control_path else {
            return HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: preview_bytes,
                detail: "未检测到 Inphic MI_02&COL04 写通道，仅保存在本地配置".to_string(),
            };
        };

        let cached = telemetry_daemon::get_cached_inphic_telemetry();
        if let Err(reason) = protocols::inphic_in10::validate_inphic_write_preconditions(
            vid,
            pid,
            cpath,
            cached.model_id,
        ) {
            return HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: preview_bytes,
                detail: reason,
            };
        }

        if let Ok(dir) = portable_data_dir() {
            if let Ok(mut guard) = active_mouse_cfg_lock().lock() {
                match guard.as_ref() {
                    None => {
                        let _ = safety::save_backup(
                            &dir,
                            "inphic_0x1d57_0xfa65",
                            "mouse",
                            "initial_baseline",
                            config.clone(),
                        );
                    }
                    Some(prev) if prev != &config => {
                        let _ = safety::save_backup(
                            &dir,
                            "inphic_0x1d57_0xfa65",
                            "mouse",
                            "pre_write",
                            prev.clone(),
                        );
                    }
                    _ => {}
                }
                *guard = Some(config.clone());
            }
        }

        let api = match HidApi::new() {
            Ok(a) => a,
            Err(e) => {
                return HidDispatchResult {
                    dispatched_to_hardware: false,
                    report_bytes: preview_bytes,
                    detail: format!("Windows 原生 HID 初始化失败: {e}"),
                };
            }
        };

        return match protocols::inphic_in10::apply_inphic_in10_config(&api, cpath, &config) {
            Ok(bytes) => HidDispatchResult {
                dispatched_to_hardware: true,
                report_bytes: bytes,
                detail: "已通过 Inphic 64B HID (0x04) 下发 DPI / 回报率 / LOD / 去抖并完成双阶段 ACK 校验"
                    .to_string(),
            },
            Err(err) => HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: preview_bytes,
                detail: err,
            },
        };
    }

    HidDispatchResult {
        dispatched_to_hardware: false,
        report_bytes: preview_bytes,
        detail: "已保存鼠标调校参数至便携配置目录（当前设备处于安全只读模式）".to_string(),
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
async fn rollback_mouse_config(
    vendor_id: Option<String>,
    product_id: Option<String>,
    vendor_control_path: Option<String>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if safety::is_readonly_mode() {
            return Err("全局只读安全模式已开启，已拦截鼠标配置回滚写入".to_string());
        }

        let dir = portable_data_dir()?;
        let current = active_mouse_cfg_lock().lock().ok().and_then(|g| g.clone());
        let backups = safety::list_backups(&dir, Some("inphic_0x1d57_0xfa65"));
        let target = backups
            .iter()
            .find(|b| current.as_ref().map(|c| c != &b.payload).unwrap_or(true))
            .or_else(|| backups.first())
            .cloned()
            .ok_or_else(|| "暂无可回滚的 Inphic 鼠标配置历史快照".to_string())?;

        let mut restored = target.payload.clone();
        if let Some(obj) = restored.as_object_mut() {
            if obj.get("vendorId").is_none() {
                if let Some(v) = vendor_id {
                    obj.insert("vendorId".to_string(), serde_json::Value::String(v));
                }
            }
            if obj.get("productId").is_none() {
                if let Some(p) = product_id {
                    obj.insert("productId".to_string(), serde_json::Value::String(p));
                }
            }
            if obj.get("vendorControlPath").and_then(|v| v.as_str()).is_none() {
                if let Some(cp) = vendor_control_path {
                    obj.insert(
                        "vendorControlPath".to_string(),
                        serde_json::Value::String(cp),
                    );
                }
            }
        }

        if let Ok(mut guard) = active_mouse_cfg_lock().lock() {
            *guard = Some(restored.clone());
        }

        let dispatch = dispatch_mouse_blocking(restored.clone());
        Ok(serde_json::json!({
            "mouseConfig": restored,
            "dispatch": dispatch,
            "source": target.source,
            "detail": if dispatch.dispatched_to_hardware {
                "已从原子快照回滚 Inphic 鼠标参数并完成双 ACK 校验"
            } else {
                dispatch.detail.as_str()
            }
        }))
    })
    .await
    .map_err(|e| e.to_string())?
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

        if let Ok(dir) = portable_data_dir() {
            if let Ok(mut guard) = active_kbd_cfg_lock().lock() {
                match guard.as_ref() {
                    None => {
                        let _ = safety::save_backup(
                            &dir,
                            "keyboard_soft_driver",
                            "keyboard",
                            "initial_baseline",
                            config.clone(),
                        );
                    }
                    Some(prev) if prev != &config => {
                        let _ = safety::save_backup(
                            &dir,
                            "keyboard_soft_driver",
                            "keyboard",
                            "pre_write",
                            prev.clone(),
                        );
                    }
                    _ => {}
                }
                *guard = Some(config.clone());
            }
        }

        let status = keyboard_engine::update_config(&config);
        if safety::is_readonly_mode() {
            return HidDispatchResult {
                dispatched_to_hardware: false,
                report_bytes: report.to_vec(),
                detail: "全局只读安全模式已开启：已保存键盘配置，软驱动暂不拦截物理按键".to_string(),
            };
        }

        HidDispatchResult {
            dispatched_to_hardware: true,
            report_bytes: report.to_vec(),
            detail: format!(
                "键盘软驱动已同步（层 {} · 映射 {} 键 · 零 Flash 写入风险）",
                status.active_layer.to_uppercase(),
                status.active_bindings_count
            ),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_macros_to_engine(macros: serde_json::Value) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || keyboard_engine::sync_macros(&macros))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_keyboard_engine_enabled(enabled: bool) -> Result<KeyboardEngineStatus, String> {
    tauri::async_runtime::spawn_blocking(move || keyboard_engine::set_engine_enabled(enabled))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_keyboard_engine_status() -> Result<KeyboardEngineStatus, String> {
    tauri::async_runtime::spawn_blocking(keyboard_engine::get_status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn emergency_reset_keyboard_engine() -> Result<KeyboardEngineStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        keyboard_engine::emergency_reset("已手动触发紧急停用，键盘软驱动已切回原厂直通")
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rollback_keyboard_config() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let dir = portable_data_dir()?;
        let current = active_kbd_cfg_lock().lock().ok().and_then(|g| g.clone());
        let backups = safety::list_backups(&dir, Some("keyboard_soft_driver"));
        let target = backups
            .iter()
            .find(|b| current.as_ref().map(|c| c != &b.payload).unwrap_or(true))
            .or_else(|| backups.first())
            .cloned()
            .ok_or_else(|| "暂无可回滚的键盘配置备份快照".to_string())?;

        if let Ok(mut guard) = active_kbd_cfg_lock().lock() {
            *guard = Some(target.payload.clone());
        }
        keyboard_engine::update_config(&target.payload);
        Ok(target.payload)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_thx_status() -> Result<protocols::thx_grpc::ThxStatus, String> {
    Ok(protocols::thx_grpc::get_status().await)
}

#[tauri::command]
async fn apply_audio_eq(config: serde_json::Value) -> Result<HidDispatchResult, String> {
    let mut payload = Vec::with_capacity(10);
    if let Some(bands) = config.get("bands").and_then(|v| v.as_array()) {
        for band in bands.iter().take(10) {
            let db = band.as_f64().unwrap_or(0.0).clamp(-12.0, 12.0);
            let encoded = ((db + 12.0) * 10.0).round() as u8;
            payload.push(encoded);
        }
    }
    let report = hid_protocol::build_feature_report(hid_protocol::CMD_SET_AUDIO_EQ, &payload);

    if safety::is_readonly_mode() {
        return Ok(HidDispatchResult {
            dispatched_to_hardware: false,
            report_bytes: report.to_vec(),
            detail: "全局只读安全模式已开启：已阻止 THX 服务写入，仅保存在本地配置".to_string(),
        });
    }

    match protocols::thx_grpc::capture_headphones_snapshot().await {
        Ok(live_snapshot) => {
            if let Ok(dir) = portable_data_dir() {
                if let Ok(mut guard) = active_audio_cfg_lock().lock() {
                    match guard.as_ref() {
                        None => {
                            let snap_payload = serde_json::json!({
                                "uiConfig": config,
                                "thxSnapshot": live_snapshot,
                            });
                            let _ = safety::save_backup(
                                &dir,
                                "hecate_g5max_thx",
                                "headset",
                                "initial_baseline",
                                snap_payload,
                            );
                        }
                        Some(prev) if prev != &config => {
                            let snap_payload = serde_json::json!({
                                "uiConfig": prev,
                                "thxSnapshot": live_snapshot,
                            });
                            let _ = safety::save_backup(
                                &dir,
                                "hecate_g5max_thx",
                                "headset",
                                "pre_write",
                                snap_payload,
                            );
                        }
                        _ => {}
                    }
                    *guard = Some(config.clone());
                }
            }

            match protocols::thx_grpc::apply_ui_audio_config(&config, Some(&live_snapshot)).await {
                Ok(_) => Ok(HidDispatchResult {
                    dispatched_to_hardware: true,
                    report_bytes: report.to_vec(),
                    detail: "已实时同步 10 段 EQ 与 THX 空间声场至 HECATE G5 Max 服务".to_string(),
                }),
                Err(err) => Ok(HidDispatchResult {
                    dispatched_to_hardware: false,
                    report_bytes: report.to_vec(),
                    detail: format!("THX 音效写入未完成（已保留本地曲线与 EqualizerAPO 导出）: {err}"),
                }),
            }
        }
        Err(err) => Ok(HidDispatchResult {
            dispatched_to_hardware: false,
            report_bytes: report.to_vec(),
            detail: format!("本地 THX 服务暂不可用，已保存 EQ 曲线（可导出 EqualizerAPO）: {err}"),
        }),
    }
}

#[tauri::command]
async fn rollback_audio_config() -> Result<serde_json::Value, String> {
    if safety::is_readonly_mode() {
        return Err("全局只读安全模式已开启，已拦截 THX 音效回滚写入".to_string());
    }

    let (target, restored_snap, ui_cfg) = tauri::async_runtime::spawn_blocking(|| {
        let dir = portable_data_dir()?;
        let current = active_audio_cfg_lock().lock().ok().and_then(|g| g.clone());
        let backups = safety::list_backups(&dir, Some("hecate_g5max_thx"));
        let target = backups
            .iter()
            .find(|b| {
                let b_ui = b.payload.get("uiConfig");
                match (current.as_ref(), b_ui) {
                    (Some(cur), Some(bu)) => cur != bu,
                    _ => true,
                }
            })
            .or_else(|| backups.first())
            .cloned()
            .ok_or_else(|| "暂无可回滚的 HECATE THX 音效历史快照".to_string())?;

        let thx_snap: protocols::thx_grpc::ThxAudioSnapshot = serde_json::from_value(
            target
                .payload
                .get("thxSnapshot")
                .cloned()
                .ok_or_else(|| "备份快照缺少 thxSnapshot 字段".to_string())?,
        )
        .map_err(|e| format!("解析 THX 备份快照失败: {e}"))?;

        let ui_cfg = target
            .payload
            .get("uiConfig")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Ok::<_, String>((target, thx_snap, ui_cfg))
    })
    .await
    .map_err(|e| e.to_string())??;

    protocols::thx_grpc::restore_thx_snapshot(&restored_snap).await?;
    if !ui_cfg.is_null() {
        if let Ok(mut guard) = active_audio_cfg_lock().lock() {
            *guard = Some(ui_cfg.clone());
        }
    }

    Ok(serde_json::json!({
        "uiConfig": ui_cfg,
        "thxSnapshot": restored_snap,
        "restoredToThx": true,
        "source": target.source,
        "detail": "已从原子快照一键恢复 THX 10 段 EQ 与空间声场参数"
    }))
}

#[tauri::command]
async fn apply_headset_led(
    vendor_id: String,
    product_id: String,
    led_path: String,
    mode: u8,
) -> Result<HidDispatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mode_label = match mode {
            0 => "关闭",
            1 => "常亮",
            2 => "呼吸",
            3 => "幻彩循环",
            _ => {
                return Err(format!(
                    "安全护栏拦截：不支持的 HECATE 灯效模式编号 {mode}（仅允许 0..=3）"
                ))
            }
        };
        let vid = parse_hex_id(&vendor_id).unwrap_or(0);
        let pid = parse_hex_id(&product_id).unwrap_or(0);
        let api = HidApi::new().map_err(|e| format!("Windows 原生 HID 初始化失败: {e}"))?;
        let packets = protocols::hecate_g5max::apply_g5max_led_mode(&api, &led_path, vid, pid, mode)?;
        let report_bytes: Vec<u8> = packets.iter().flat_map(|p| p.iter().copied()).collect();
        Ok(HidDispatchResult {
            dispatched_to_hardware: true,
            report_bytes,
            detail: format!("HECATE 耳机 RGB 灯效已切换为：{mode_label}（已通过 0xED 协议边界校验）"),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn execute_macro(macro_def: serde_json::Value) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || keyboard_engine::run_macro_once(&macro_def))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_updater_settings() -> Result<updater::UpdaterSettingsView, String> {
    let dir = tauri::async_runtime::spawn_blocking(portable_data_dir)
        .await
        .map_err(|e| e.to_string())??;
    Ok(updater::get_updater_settings(&dir))
}

#[tauri::command]
async fn save_updater_settings(
    repo: String,
    github_token: Option<String>,
    clear_token: bool,
) -> Result<updater::UpdaterSettingsView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = portable_data_dir()?;
        updater::save_updater_settings(&dir, &repo, github_token, clear_token)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_app_update() -> Result<updater::UpdateCheckResult, String> {
    let dir = tauri::async_runtime::spawn_blocking(portable_data_dir)
        .await
        .map_err(|e| e.to_string())??;
    updater::check_for_update(&dir).await
}

#[tauri::command]
async fn download_and_apply_update(
    app: tauri::AppHandle,
    update: updater::UpdateCheckResult,
) -> Result<updater::UpdateApplyResult, String> {
    let dir = tauri::async_runtime::spawn_blocking(portable_data_dir)
        .await
        .map_err(|e| e.to_string())??;

    let app_for_cb = app.clone();
    let cb: updater::ProgressCallback = Arc::new(move |progress: updater::UpdateProgress| {
        if let (Some(win), Ok(json)) = (
            app_for_cb.get_webview_window("main"),
            serde_json::to_string(&progress),
        ) {
            let js = format!(
                "window.dispatchEvent(new CustomEvent('lumen-update-progress', {{ detail: {json} }}));"
            );
            let _ = win.eval(&js);
        }
    });

    let res = updater::download_and_apply_update(&dir, &update, Some(cb)).await?;
    if res.will_restart {
        let app_for_exit = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            app_for_exit.exit(0);
        });
    }
    Ok(res)
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim().to_string();
    if !trimmed.starts_with("https://github.com/") {
        return Err("安全护栏拦截：仅允许打开 https://github.com/ 链接".to_string());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &trimmed])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("打开外部浏览器失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = trimmed;
        Err("当前平台不支持打开外部浏览器".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            // Ensure the Windows taskbar & titlebar icon is always mounted at runtime as well as in PE resources
            if let (Some(window), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = window.set_icon(icon.clone());
            }

            let handle = app.handle().clone();
            keyboard_engine::set_notice_callback(Arc::new(move |msg: String| {
                let status = keyboard_engine::get_status();
                if let (Some(win), Ok(json)) = (
                    handle.get_webview_window("main"),
                    serde_json::to_string(&status),
                ) {
                    let js = format!(
                        "window.dispatchEvent(new CustomEvent('lumen-keyboard-status', {{ detail: {json} }}));"
                    );
                    let _ = win.eval(&js);
                }
                let _ = msg;
            }));
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
            rollback_mouse_config,
            apply_keyboard_config,
            sync_macros_to_engine,
            set_keyboard_engine_enabled,
            get_keyboard_engine_status,
            emergency_reset_keyboard_engine,
            rollback_keyboard_config,
            get_thx_status,
            apply_audio_eq,
            rollback_audio_config,
            apply_headset_led,
            execute_macro,
            get_updater_settings,
            save_updater_settings,
            check_app_update,
            download_and_apply_update,
            open_external_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LumenHID Tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            keyboard_engine::shutdown();
        }
    });
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
