//! Vendor protocol fingerprinting, multi-channel composite HID aggregation, strict system-endpoint filtering,
//! and safe read-only hardware battery / firmware telemetry dispatch for real Windows peripherals.

use crate::protocols::hecate_g5max::{
    query_g5max_status, PID_G5MAX_WIRED, PID_G5MAX_WIRELESS, VID_HECATE,
};
use crate::protocols::inphic_in10::{PID_KP8K_DONGLE, VID_INPHIC};
use crate::telemetry_daemon::{
    ensure_inphic_listener, get_cached_inphic_telemetry, TelemetryCallback,
};
use crate::win_battery::{extract_ble_mac, WinBluetoothDeviceInfo};
use hidapi::{DeviceInfo, HidApi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeripheral {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub manufacturer: String,
    pub interface_number: i32,
    pub device_path: String,
    pub vendor_control_path: Option<String>,
    pub telemetry_path: Option<String>,
    pub led_path: Option<String>,
    pub protocol: String,
    pub category: String,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub battery_source: String, // "windows-bt" | "hid-vendor" | "wired" | "unavailable"
    pub firmware_version: Option<String>,
    pub protocol_support: String, // "native-full" | "soft-driver-safe" | "native-battery-readonly" | "system-apo"
    pub protocol_note: String,
    pub max_polling_hz: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct VendorFingerprint {
    pub display_name: &'static str,
    pub manufacturer: &'static str,
    pub category: &'static str,
    pub profile_id: &'static str,
    pub protocol_support: &'static str,
    pub protocol_note: &'static str,
    pub max_polling_hz: u16,
}

/// Known physical peripheral fingerprints (VID/PID or product name signatures).
pub fn lookup_vendor_fingerprint(vid: u16, pid: u16, product_name: &str) -> Option<VendorFingerprint> {
    let lower = product_name.to_ascii_lowercase();

    // 1. 英菲克 Inphic IN10 / KP 8K (VID 0x1D57 / PID 0xFA65)
    if (vid == VID_INPHIC && pid == PID_KP8K_DONGLE)
        || (lower.contains("inphic") && (lower.contains("kp") || lower.contains("in10")))
    {
        return Some(VendorFingerprint {
            display_name: "Inphic IN10 / KP 8K",
            manufacturer: "英菲克 Inphic",
            category: "mouse",
            profile_id: "inphic_in10_8k",
            protocol_support: "native-full",
            protocol_note: "已识别英菲克 IN10 / KP 8K (0x1D57:0xFA65)；已锁定 MI_02&COL03 遥测通道读取实时电量，并通过 MI_02&COL04 (64B 0x04 协议 + 双 ACK) 安全调校。",
            max_polling_hz: 8000,
        });
    }

    // 2. 凌豹 K98PRO 5.0 蓝牙模式 (VID 0x258A / PID 0x0040)
    if (vid == 0x258A && pid == 0x0040)
        || lower.contains("k98pro")
        || lower.contains("k98 pro")
        || lower.contains("leobog")
    {
        return Some(VendorFingerprint {
            display_name: "凌豹 K98PRO 5.0",
            manufacturer: "凌豹 / Sinowealth",
            category: "keyboard",
            profile_id: "sinowealth_k98pro_bt",
            protocol_support: "soft-driver-safe",
            protocol_note: "已识别凌豹 K98PRO 5.0 (中颖 0x258A:0x0040)；电量通过 Windows 原生 BLE MAC 实时获取。为严防中颖 Flash 盲写变砖，常用改键/宏/SOCD/锁Win 均由软驱动安全接管。",
            max_polling_hz: 1000,
        });
    }

    // 3. 凌豹 K98PRO / 中颖方案 USB/2.4G 模式 (VID 0x258A / PID 0x019D)
    if vid == 0x258A && pid == 0x019D {
        return Some(VendorFingerprint {
            display_name: "凌豹 K98PRO (USB/2.4G)",
            manufacturer: "凌豹 / Sinowealth",
            category: "keyboard",
            profile_id: "sinowealth_k98pro_usb",
            protocol_support: "soft-driver-safe",
            protocol_note: "已识别中颖方案键盘 (0x258A:0x019D)；防变砖护栏已永久阻断未知 Flash 写入，常用改键、多层切换、宏录制、SOCD 与防抖由软驱动引擎安全实现。",
            max_polling_hz: 1000,
        });
    }

    // 4. 漫步者 HECATE G5 Max 2.4G 无线版 (VID 0x35BB / PID 0xA00B)
    if vid == VID_HECATE && pid == PID_G5MAX_WIRELESS {
        return Some(VendorFingerprint {
            display_name: "漫步者 HECATE G5 Max 无线版",
            manufacturer: "漫步者 Edifier HECATE",
            category: "headset",
            profile_id: "edifier_g5_max",
            protocol_support: "native-full",
            protocol_note: "已识别漫步者 HECATE G5 Max 无线版 (0x35BB:0xA00B)；已锁定 MI_03&COL03 直读实时电量与固件版本，10 段 EQ 与 THX 空间音效可直连本机 THX 服务。",
            max_polling_hz: 1000,
        });
    }

    // 5. 漫步者 HECATE G5 Max USB 有线版 (VID 0x35BB / PID 0xA211)
    if vid == VID_HECATE && pid == PID_G5MAX_WIRED {
        return Some(VendorFingerprint {
            display_name: "漫步者 HECATE G5 Max 有线版",
            manufacturer: "漫步者 Edifier HECATE",
            category: "headset",
            profile_id: "edifier_g5_max_wired",
            protocol_support: "native-full",
            protocol_note: "已识别漫步者 HECATE G5 Max 有线版 (0x35BB:0xA211)；已锁定 MI_03&COL02 状态通道与本机 THX gRPC 服务。",
            max_polling_hz: 1000,
        });
    }

    // 6. 漫步者 HECATE G5 Max 蓝牙模式（名称特征兜底）
    if lower.contains("g5 max")
        || lower.contains("g5max")
        || (lower.contains("hecate") && lower.contains("g5"))
        || (lower.contains("edifier") && lower.contains("g5"))
    {
        return Some(VendorFingerprint {
            display_name: "漫步者 HECATE G5 Max",
            manufacturer: "漫步者 Edifier HECATE",
            category: "headset",
            profile_id: "edifier_g5_max_bt",
            protocol_support: "system-apo",
            protocol_note: "已识别漫步者 HECATE G5 Max 蓝牙模式；通过 Windows 原生 PnP 读取实时电量，支持导出 EqualizerAPO 或配合 2.4G 模式直连 THX 服务。",
            max_polling_hz: 1000,
        });
    }

    // Generic Edifier / HECATE headsets
    if lower.contains("hecate") || lower.contains("edifier") || vid == 0x2D99 || vid == VID_HECATE {
        return Some(VendorFingerprint {
            display_name: "漫步者 HECATE 音频外设",
            manufacturer: "漫步者 Edifier",
            category: "headset",
            profile_id: "edifier_generic",
            protocol_support: "system-apo",
            protocol_note: "已识别漫步者音频设备；支持原生蓝牙电量读取与 EqualizerAPO 全局 EQ 配置导出。",
            max_polling_hz: 1000,
        });
    }

    None
}

/// Strictly excludes built-in motherboard/GPU audio endpoints, touchpads, system ACPI/control nodes,
/// and virtual software devices (such as `Realtek 高清音频输出 (扬声器/耳麦)`).
pub fn is_system_or_virtual_endpoint(
    name: &str,
    manufacturer: &str,
    device_path: &str,
    usage_page: Option<u16>,
    usage: Option<u16>,
) -> bool {
    let n = name.to_ascii_lowercase();
    let m = manufacturer.to_ascii_lowercase();
    let p = device_path.to_ascii_lowercase();
    let combined = format!("{n} {m}");

    // Always allow explicitly recognized external gaming/desktop peripherals
    let known_brand_allowlist = [
        "inphic",
        "英菲克",
        "hecate",
        "edifier",
        "漫步者",
        "g5 max",
        "g5max",
        "k98pro",
        "k98 pro",
        "leobog",
        "凌豹",
        "sinowealth",
        "sino wealth",
        "logitech",
        "razer",
        "steelseries",
        "hyperx",
        "corsair",
        "keychron",
        "wooting",
        "vgn",
        "atk",
        "finalmouse",
        "pulsar",
        "lamzu",
        "sennheiser",
        "sony",
        "bose",
        "rog",
        "akko",
        "varmilo",
        "nuphy",
        "dareu",
        "rapoo",
        "mchose",
        "zaopin",
        "ajazz",
        "aula",
        "狼蛛",
    ];
    if known_brand_allowlist.iter().any(|kw| combined.contains(kw)) {
        if !n.contains("realtek") && !n.contains("高清音频") {
            return false;
        }
    }

    let blocked_keywords = [
        "realtek",
        "high definition audio",
        "高清音频",
        "扬声器",
        "麦克风阵列",
        "立体声混音",
        "hdmi",
        "displayport",
        "nvidia",
        "amd high definition",
        "intel(r) display",
        "intel smart sound",
        "conexant",
        "nahimic",
        "dolby",
        "voicemeeter",
        "vb-audio",
        "virtual audio",
        "steam streaming",
        "touchpad",
        "precision touchpad",
        "elan",
        "synaptics",
        "alps",
        "i2c hid",
        "acpi",
        "system control",
        "converted portable",
        "root hub",
        "usb hub",
        "generic usb hub",
        "bluetooth adapter",
        "bluetooth radio",
        "intel(r) wireless bluetooth",
        "microsoft bluetooth enumerator",
        "avrcp transport",
        "hands-free ag",
        "hid-compliant system",
        "hid-compliant vendor-defined device",
        "hid-compliant consumer control",
        "usb 输入设备",
        "virtual",
        "vmware",
        "hyper-v",
        "parsec",
        "vmulti",
        "sunshine",
        "oculus",
        "camera",
        "webcam",
        "fingerprint",
        "sensor",
        "battery",
        "ups",
    ];

    if blocked_keywords.iter().any(|kw| combined.contains(kw)) {
        return true;
    }

    if p.contains("acpi#") || p.contains("hid#elan") || p.contains("hid#syn") || p.contains("hid#msft") {
        return true;
    }

    if let (Some(up), Some(u)) = (usage_page, usage) {
        if up == 0x0D || up == 0x20 || up == 0x84 || up == 0x85 {
            return true;
        }
        if up == 0x01 && (u == 0x80 || u == 0x04 || u == 0x05) {
            return true;
        }
    }

    false
}

/// Classifies transport protocol from Windows HID device path & product name.
pub fn infer_protocol_from_path(device_path: &str, product_name: &str) -> String {
    let path_lower = device_path.to_ascii_lowercase();
    let name_lower = product_name.to_ascii_lowercase();
    if path_lower.contains("bthle")
        || path_lower.contains("bthenum")
        || path_lower.contains("{00001812")
        || name_lower.contains("bluetooth")
        || name_lower.contains("ble")
        || name_lower.contains("5.0")
        || name_lower.contains("5.3")
    {
        return "bluetooth".to_string();
    }
    if path_lower.contains("vid_35bb&pid_a00b")
        || name_lower.contains("2.4g")
        || name_lower.contains("receiver")
        || name_lower.contains("dongle")
        || name_lower.contains("wireless")
        || name_lower.contains("无线")
        || name_lower.contains("lightspeed")
        || name_lower.contains("8k")
    {
        return "2.4g".to_string();
    }
    "usb".to_string()
}

struct CompositeAccumulator {
    vid: u16,
    pid: u16,
    best_name: String,
    best_manufacturer: String,
    best_path: String,
    best_interface: i32,
    vendor_control_path: Option<String>,
    telemetry_path: Option<String>,
    led_path: Option<String>,
    protocol: String,
    has_mouse_usage: bool,
    has_keyboard_usage: bool,
    has_audio_usage: bool,
}

impl CompositeAccumulator {
    fn resolve_category(&self, fp: Option<VendorFingerprint>) -> String {
        if let Some(f) = fp {
            return f.category.to_string();
        }
        let lower = self.best_name.to_ascii_lowercase();
        if lower.contains("headset")
            || lower.contains("headphone")
            || lower.contains("ear")
            || lower.contains("hecate")
            || lower.contains("edifier")
            || lower.contains("audio")
        {
            return "headset".to_string();
        }
        if lower.contains("mouse") || lower.contains("鼠标") || lower.contains("gmice") {
            return "mouse".to_string();
        }
        if lower.contains("keyboard") || lower.contains("kbd") || lower.contains("键盘") {
            return "keyboard".to_string();
        }
        if self.has_mouse_usage {
            return "mouse".to_string();
        }
        if self.has_keyboard_usage {
            return "keyboard".to_string();
        }
        if self.has_audio_usage {
            return "headset".to_string();
        }
        "mouse".to_string()
    }

    fn record_channel_path(&mut self, path_str: &str, up: u16, u: u16) {
        let lower = path_str.to_ascii_lowercase();

        // Inphic IN10 / KP 8K (0x1D57:0xFA65)
        if self.vid == VID_INPHIC && self.pid == PID_KP8K_DONGLE {
            if (up == 0x000A && u == 0x0000) || lower.contains("mi_02&col03") {
                if self.telemetry_path.is_none() {
                    self.telemetry_path = Some(path_str.to_string());
                }
                return;
            }
            if (up == 0xFF00 && u == 0x0001) || lower.contains("mi_02&col04") {
                if self.vendor_control_path.is_none() {
                    self.vendor_control_path = Some(path_str.to_string());
                }
                return;
            }
        }

        // Edifier HECATE G5 Max 2.4G (0x35BB:0xA00B)
        if self.vid == VID_HECATE && self.pid == PID_G5MAX_WIRELESS {
            if lower.contains("mi_03&col03") {
                self.vendor_control_path = Some(path_str.to_string());
                return;
            }
            if lower.contains("mi_03&col04") {
                self.led_path = Some(path_str.to_string());
                return;
            }
            return;
        }

        // Edifier HECATE G5 Max Wired (0x35BB:0xA211)
        if self.vid == VID_HECATE && self.pid == PID_G5MAX_WIRED {
            if lower.contains("mi_03&col02") {
                self.vendor_control_path = Some(path_str.to_string());
                return;
            }
            if lower.contains("mi_03&col03") {
                self.led_path = Some(path_str.to_string());
                return;
            }
            return;
        }

        if up >= 0xFF00 && self.vendor_control_path.is_none() {
            self.vendor_control_path = Some(path_str.to_string());
        }
    }
}

pub struct VendorBatteryProbeResult {
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub battery_source: String,
    pub firmware_version: Option<String>,
    pub dynamic_name: Option<String>,
}

/// Safe read-only battery & firmware telemetry probe.
/// Never writes to SinoWealth (0x258A) or performs risky Feature Report writes.
pub fn probe_vendor_telemetry(
    api: &HidApi,
    control_path: Option<&str>,
    telemetry_path: Option<&str>,
    profile_id: &str,
    protocol: &str,
    on_event: Option<&TelemetryCallback>,
) -> VendorBatteryProbeResult {
    match profile_id {
        "inphic_in10_8k" => {
            if let Some(tpath) = telemetry_path {
                ensure_inphic_listener(tpath.to_string(), on_event.cloned());
            }
            let cached = get_cached_inphic_telemetry();
            if let Some(pct) = cached.battery_percent {
                return VendorBatteryProbeResult {
                    battery_percent: Some(pct),
                    charging: cached.charging,
                    battery_source: "hid-vendor".to_string(),
                    firmware_version: Some("KP8K-COL03".to_string()),
                    dynamic_name: cached.model_name,
                };
            }
            VendorBatteryProbeResult {
                battery_percent: None,
                charging: false,
                battery_source: "unavailable".to_string(),
                firmware_version: Some("KP8K-COL03".to_string()),
                dynamic_name: cached.model_name,
            }
        }
        "edifier_g5_max" | "edifier_g5_max_wired" => {
            let is_wireless = profile_id == "edifier_g5_max";
            if let Some(cpath) = control_path {
                if let Some(status) = query_g5max_status(api, cpath, is_wireless) {
                    let fw = format!("HS {} / DG {}", status.device_fw, status.dongle_fw);
                    return VendorBatteryProbeResult {
                        battery_percent: status.battery_percent,
                        charging: status.charging,
                        battery_source: if status.battery_percent.is_some() {
                            "hid-vendor".to_string()
                        } else if !is_wireless {
                            "wired".to_string()
                        } else {
                            "unavailable".to_string()
                        },
                        firmware_version: Some(fw),
                        dynamic_name: None,
                    };
                }
            }
            VendorBatteryProbeResult {
                battery_percent: None,
                charging: false,
                battery_source: if is_wireless {
                    "unavailable".to_string()
                } else {
                    "wired".to_string()
                },
                firmware_version: None,
                dynamic_name: None,
            }
        }
        // Explicitly keep SinoWealth (0x258A) read-only and never probe unknown Feature Reports that could stall firmware
        "sinowealth_k98pro_bt" | "sinowealth_k98pro_usb" => VendorBatteryProbeResult {
            battery_percent: None,
            charging: false,
            battery_source: if protocol == "usb" {
                "wired".to_string()
            } else {
                "unavailable".to_string()
            },
            firmware_version: Some("SOFT-DRV".to_string()),
            dynamic_name: None,
        },
        _ => VendorBatteryProbeResult {
            battery_percent: None,
            charging: false,
            battery_source: if protocol == "usb" {
                "wired".to_string()
            } else {
                "unavailable".to_string()
            },
            firmware_version: None,
            dynamic_name: None,
        },
    }
}

/// Aggregates all raw HID interfaces into deduplicated physical peripherals, fuses Windows
/// Bluetooth PnP battery telemetry (by BLE MAC, VID/PID, or name), and appends pure BT devices.
pub fn aggregate_real_peripherals(
    api: &HidApi,
    bt_devices: &[WinBluetoothDeviceInfo],
    on_event: Option<&TelemetryCallback>,
) -> Vec<DiscoveredPeripheral> {
    let mut grouped: HashMap<(u16, u16), CompositeAccumulator> = HashMap::new();

    for dev in api.device_list() {
        let vid = dev.vendor_id();
        let pid = dev.product_id();
        if vid == 0 || pid == 0 {
            continue;
        }
        let raw_name = dev
            .product_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        let raw_mfr = dev
            .manufacturer_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string();
        let path_str = dev.path().to_string_lossy().to_string();
        let up = dev.usage_page();
        let u = dev.usage();

        if is_system_or_virtual_endpoint(&raw_name, &raw_mfr, &path_str, Some(up), Some(u)) {
            continue;
        }

        let fp = lookup_vendor_fingerprint(vid, pid, &raw_name);
        let is_primary_hid = (up == 0x01 && (u == 0x02 || u == 0x06)) || up >= 0xFF00 || up == 0x0C;
        if fp.is_none() && !is_primary_hid {
            continue;
        }

        let entry = grouped.entry((vid, pid)).or_insert_with(|| CompositeAccumulator {
            vid,
            pid,
            best_name: if raw_name.is_empty() {
                format!("HID 外设 {:04X}:{:04X}", vid, pid)
            } else {
                raw_name.clone()
            },
            best_manufacturer: if raw_mfr.is_empty() {
                "Standard HID".to_string()
            } else {
                raw_mfr.clone()
            },
            best_path: path_str.clone(),
            best_interface: dev.interface_number(),
            vendor_control_path: None,
            telemetry_path: None,
            led_path: None,
            protocol: infer_protocol_from_path(&path_str, &raw_name),
            has_mouse_usage: false,
            has_keyboard_usage: false,
            has_audio_usage: false,
        });

        if !raw_name.is_empty() && entry.best_name.starts_with("HID 外设") {
            entry.best_name = raw_name.clone();
        }
        if !raw_mfr.is_empty() && entry.best_manufacturer == "Standard HID" {
            entry.best_manufacturer = raw_mfr.clone();
        }

        if up == 0x01 && u == 0x02 {
            entry.has_mouse_usage = true;
        }
        if up == 0x01 && u == 0x06 {
            entry.has_keyboard_usage = true;
        }
        if up == 0x0C {
            entry.has_audio_usage = true;
        }

        entry.record_channel_path(&path_str, up, u);

        if infer_protocol_from_path(&path_str, &entry.best_name) == "bluetooth" {
            entry.protocol = "bluetooth".to_string();
        }
    }

    let mut results: Vec<DiscoveredPeripheral> = Vec::new();
    let mut matched_bt_keys: Vec<String> = Vec::new();

    for ((vid, pid), acc) in grouped {
        let fp = lookup_vendor_fingerprint(vid, pid, &acc.best_name);
        if fp.is_none() && !acc.has_mouse_usage && !acc.has_keyboard_usage {
            continue;
        }

        let mut display_name = fp
            .map(|f| f.display_name.to_string())
            .unwrap_or_else(|| acc.best_name.clone());
        let manufacturer = fp
            .map(|f| f.manufacturer.to_string())
            .unwrap_or_else(|| acc.best_manufacturer.clone());
        let category = acc.resolve_category(fp);
        let profile_id = fp.map(|f| f.profile_id).unwrap_or("generic_hid");
        let hid_ble_mac = extract_ble_mac(&acc.best_path);

        // 1) Try Windows native Bluetooth battery match by BLE MAC -> (VID, PID) -> fuzzy name (only when BT VID/PID is absent)
        let bt_match = bt_devices.iter().find(|b| {
            let mac_match = hid_ble_mac.is_some() && b.ble_mac == hid_ble_mac;
            let vid_pid_match = b.vendor_id == Some(vid) && b.product_id == Some(pid);
            let name_lower = b.name.to_ascii_lowercase();
            let disp_lower = display_name.to_ascii_lowercase();
            let fuzzy_name = b.vendor_id.is_none()
                && ((name_lower.contains("k98") && disp_lower.contains("k98"))
                    || (name_lower.contains("inphic") && disp_lower.contains("inphic"))
                    || (name_lower.contains("g5 max") && disp_lower.contains("g5 max")));
            (mac_match || vid_pid_match || fuzzy_name) && b.battery_percent.is_some()
        });

        let (battery_percent, charging, battery_source, firmware_version) = if let Some(bm) = bt_match {
            matched_bt_keys.push(
                bm.ble_mac
                    .clone()
                    .unwrap_or_else(|| bm.name.to_ascii_lowercase()),
            );
            (
                bm.battery_percent,
                false,
                "windows-bt".to_string(),
                Some("BT-PNP".to_string()),
            )
        } else {
            let probe = probe_vendor_telemetry(
                api,
                acc.vendor_control_path.as_deref(),
                acc.telemetry_path.as_deref(),
                profile_id,
                &acc.protocol,
                on_event,
            );
            if let Some(dname) = probe.dynamic_name {
                display_name = dname;
            }
            (
                probe.battery_percent,
                probe.charging,
                probe.battery_source,
                probe.firmware_version,
            )
        };

        let protocol_support = fp
            .map(|f| f.protocol_support)
            .unwrap_or(if category == "keyboard" {
                "soft-driver-safe"
            } else {
                "native-battery-readonly"
            })
            .to_string();
        let protocol_note = fp
            .map(|f| f.protocol_note.to_string())
            .unwrap_or_else(|| {
                if category == "keyboard" {
                    "已通过 Windows 原生 HID 识别；默认禁止写入私有 Flash，通过安全软驱动引擎接管改键与宏。".to_string()
                } else {
                    "已通过 Windows 原生 HID 识别；通用外设默认启用安全只读与本地便携配置保存。".to_string()
                }
            });
        let max_polling_hz = fp.map(|f| f.max_polling_hz).unwrap_or(1000);

        results.push(DiscoveredPeripheral {
            vendor_id: format!("0x{:04X}", vid),
            product_id: format!("0x{:04X}", pid),
            product_name: display_name,
            manufacturer,
            interface_number: acc.best_interface,
            device_path: acc.best_path,
            vendor_control_path: acc.vendor_control_path,
            telemetry_path: acc.telemetry_path,
            led_path: acc.led_path,
            protocol: acc.protocol,
            category,
            battery_percent,
            charging,
            battery_source,
            firmware_version,
            protocol_support,
            protocol_note,
            max_polling_hz,
        });
    }

    // 2) Append pure Bluetooth peripherals (like sleeping K98PRO 5.0 or Bluetooth headsets)
    let mut seen_bt_keys: Vec<String> = results
        .iter()
        .map(|r| format!("{}:{}", r.vendor_id, r.product_id))
        .collect();

    for bt in bt_devices {
        let fp = lookup_vendor_fingerprint(
            bt.vendor_id.unwrap_or(0),
            bt.product_id.unwrap_or(0),
            &bt.name,
        );
        if bt.battery_percent.is_none() && fp.is_none() {
            continue;
        }
        if is_system_or_virtual_endpoint(&bt.name, "", &bt.hardware_id, None, None) {
            continue;
        }
        let bt_key = bt
            .ble_mac
            .clone()
            .unwrap_or_else(|| bt.name.to_ascii_lowercase());
        if matched_bt_keys.iter().any(|m| m == &bt_key) {
            continue;
        }

        let vid_str = bt
            .vendor_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_else(|| "BT-PNP".to_string());
        let pid_str = bt
            .product_id
            .map(|p| format!("0x{:04X}", p))
            .unwrap_or_else(|| "BT-AUDIO".to_string());
        let vp_key = format!("{vid_str}:{pid_str}");
        let display_name = fp
            .map(|f| f.display_name.to_string())
            .unwrap_or_else(|| bt.name.clone());

        if vid_str != "BT-PNP" && seen_bt_keys.iter().any(|k| k == &vp_key) {
            continue;
        }
        seen_bt_keys.push(vp_key);

        let category = fp
            .map(|f| f.category.to_string())
            .unwrap_or_else(|| {
                let bt_lower = bt.name.to_ascii_lowercase();
                if bt_lower.contains("mouse") || bt_lower.contains("鼠标") {
                    "mouse".to_string()
                } else if bt_lower.contains("key") || bt_lower.contains("kbd") || bt_lower.contains("键盘") || bt_lower.contains("k98") {
                    "keyboard".to_string()
                } else {
                    "headset".to_string()
                }
            });

        results.push(DiscoveredPeripheral {
            vendor_id: vid_str,
            product_id: pid_str,
            product_name: display_name,
            manufacturer: fp
                .map(|f| f.manufacturer.to_string())
                .unwrap_or_else(|| "Windows 蓝牙设备".to_string()),
            interface_number: -1,
            device_path: bt.hardware_id.clone(),
            vendor_control_path: None,
            telemetry_path: None,
            led_path: None,
            protocol: "bluetooth".to_string(),
            category,
            battery_percent: bt.battery_percent,
            charging: false,
            battery_source: if bt.battery_percent.is_some() {
                "windows-bt".to_string()
            } else {
                "unavailable".to_string()
            },
            firmware_version: Some("BT-PNP".to_string()),
            protocol_support: fp
                .map(|f| f.protocol_support)
                .unwrap_or("system-apo")
                .to_string(),
            protocol_note: fp
                .map(|f| f.protocol_note.to_string())
                .unwrap_or_else(|| {
                    "通过 Windows 原生蓝牙 PnP 发现并读取实时电量；音频设备支持导出 EqualizerAPO 全局 10 段 EQ。".to_string()
                }),
            max_polling_hz: fp.map(|f| f.max_polling_hz).unwrap_or(1000),
        });
    }

    results.sort_by(|a, b| {
        let rank = |cat: &str| match cat {
            "mouse" => 0,
            "keyboard" => 1,
            "headset" => 2,
            _ => 3,
        };
        rank(&a.category)
            .cmp(&rank(&b.category))
            .then_with(|| a.product_name.cmp(&b.product_name))
    });

    results
}

#[allow(dead_code)]
fn _unused_device_info_check(_d: &DeviceInfo) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_out_realtek_and_system_endpoints() {
        assert!(is_system_or_virtual_endpoint(
            "Realtek 高清音频输出 (扬声器/耳麦)",
            "Realtek",
            r"SWD\MMDEVAPI\REALTEK",
            None,
            None
        ));
        assert!(is_system_or_virtual_endpoint(
            "ELAN Precision Touchpad",
            "ELAN",
            r"\\?\hid#elan0634",
            Some(0x0D),
            Some(0x05)
        ));
        assert!(!is_system_or_virtual_endpoint(
            "HECATE G5 Max",
            "Edifier",
            r"BTHENUM\Dev_112233",
            None,
            None
        ));
        assert!(!is_system_or_virtual_endpoint(
            "Inphic KP 8K",
            "Inphic",
            r"\\?\hid#vid_1d57&pid_fa65",
            Some(0x01),
            Some(0x02)
        ));
        assert!(!is_system_or_virtual_endpoint(
            "Gaming Keyboard",
            "SINO WEALTH",
            r"\\?\hid#vid_258a&pid_019d",
            Some(0x01),
            Some(0x06)
        ));
    }

    #[test]
    fn identifies_all_user_peripherals_and_paths() {
        let inphic = lookup_vendor_fingerprint(0x1D57, 0xFA65, "Inphic KP 8K").unwrap();
        assert_eq!(inphic.category, "mouse");
        assert_eq!(inphic.max_polling_hz, 8000);
        assert_eq!(inphic.protocol_support, "native-full");

        let k98_bt = lookup_vendor_fingerprint(0x258A, 0x0040, "1.0.0").unwrap();
        assert_eq!(k98_bt.display_name, "凌豹 K98PRO 5.0");
        assert_eq!(k98_bt.category, "keyboard");
        assert_eq!(k98_bt.protocol_support, "soft-driver-safe");

        let k98_usb = lookup_vendor_fingerprint(0x258A, 0x019D, "Gaming Keyboard").unwrap();
        assert_eq!(k98_usb.display_name, "凌豹 K98PRO (USB/2.4G)");
        assert_eq!(k98_usb.category, "keyboard");
        assert_eq!(k98_usb.protocol_support, "soft-driver-safe");

        let g5_24g = lookup_vendor_fingerprint(0x35BB, 0xA00B, "HECATE G5 Max无线版").unwrap();
        assert_eq!(g5_24g.display_name, "漫步者 HECATE G5 Max 无线版");
        assert_eq!(g5_24g.category, "headset");
        assert_eq!(g5_24g.protocol_support, "native-full");

        assert_eq!(
            infer_protocol_from_path(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col03", "HECATE G5 Max无线版"),
            "2.4g"
        );
    }

    #[test]
    fn composite_accumulator_routes_inphic_and_g5max_channels_accurately() {
        let mut inphic = CompositeAccumulator {
            vid: 0x1D57,
            pid: 0xFA65,
            best_name: "Inphic KP 8K".to_string(),
            best_manufacturer: "Inphic".to_string(),
            best_path: String::new(),
            best_interface: 2,
            vendor_control_path: None,
            telemetry_path: None,
            led_path: None,
            protocol: "2.4g".to_string(),
            has_mouse_usage: true,
            has_keyboard_usage: false,
            has_audio_usage: false,
        };
        inphic.record_channel_path(r"\\?\hid#vid_1d57&pid_fa65&mi_02&col03", 0x000A, 0x0000);
        inphic.record_channel_path(r"\\?\hid#vid_1d57&pid_fa65&mi_02&col04", 0xFF00, 0x0001);
        assert_eq!(
            inphic.telemetry_path.as_deref(),
            Some(r"\\?\hid#vid_1d57&pid_fa65&mi_02&col03")
        );
        assert_eq!(
            inphic.vendor_control_path.as_deref(),
            Some(r"\\?\hid#vid_1d57&pid_fa65&mi_02&col04")
        );

        let mut g5 = CompositeAccumulator {
            vid: 0x35BB,
            pid: 0xA00B,
            best_name: "HECATE G5 Max无线版".to_string(),
            best_manufacturer: "Edifier".to_string(),
            best_path: String::new(),
            best_interface: 3,
            vendor_control_path: None,
            telemetry_path: None,
            led_path: None,
            protocol: "2.4g".to_string(),
            has_mouse_usage: false,
            has_keyboard_usage: false,
            has_audio_usage: true,
        };
        g5.record_channel_path(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col02", 0xFF02, 0x0002);
        g5.record_channel_path(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col03", 0xFF02, 0x0002);
        g5.record_channel_path(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col04", 0xFF02, 0x0002);
        assert_eq!(
            g5.vendor_control_path.as_deref(),
            Some(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col03")
        );
        assert_eq!(
            g5.led_path.as_deref(),
            Some(r"\\?\hid#vid_35bb&pid_a00b&mi_03&col04")
        );
    }

    #[test]
    #[ignore = "Read-only live Windows hardware verification"]
    fn live_readonly_scan_smoke_test() {
        let bt = crate::win_battery::query_windows_bluetooth_devices();
        let api = HidApi::new().expect("HidApi init");
        let first = aggregate_real_peripherals(&api, &bt, None);
        println!("First scan found {} peripherals:", first.len());
        for d in &first {
            println!(
                "  {} ({}:{}) proto={} cat={} bat={:?}% src={} fw={:?} ctrl={:?} tele={:?} led={:?}",
                d.product_name,
                d.vendor_id,
                d.product_id,
                d.protocol,
                d.category,
                d.battery_percent,
                d.battery_source,
                d.firmware_version,
                d.vendor_control_path,
                d.telemetry_path,
                d.led_path
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(1800));
        let second = aggregate_real_peripherals(&api, &bt, None);
        println!("After 1.8s passive listen:");
        for d in &second {
            println!(
                "  {} ({}:{}) bat={:?}% src={} fw={:?}",
                d.product_name, d.vendor_id, d.product_id, d.battery_percent, d.battery_source, d.firmware_version
            );
        }
        assert!(!second.is_empty());
    }
}
