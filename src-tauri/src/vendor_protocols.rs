//! Vendor protocol fingerprinting, composite HID aggregation, strict system-endpoint filtering,
//! and safe hardware battery / tuning report dispatch for real Windows peripherals.

use crate::win_battery::WinBluetoothDeviceInfo;
use hidapi::{DeviceInfo, HidApi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::CString;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeripheral {
    pub vendor_id: String,
    pub product_id: String,
    pub product_name: String,
    pub manufacturer: String,
    pub interface_number: i32,
    pub device_path: String,
    pub vendor_control_path: Option<String>,
    pub protocol: String,
    pub category: String,
    pub battery_percent: Option<u8>,
    pub battery_source: String, // "windows-bt" | "hid-vendor" | "wired" | "unavailable"
    pub protocol_support: String, // "native-full" | "native-battery-experimental-tuning" | "native-battery-readonly" | "system-apo"
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

    // 1. 英菲克 Inphic KP 8K (VID 0x1D57 / PID 0xFA65 — Beken / CompX 8K architecture)
    if (vid == 0x1D57 && pid == 0xFA65) || (lower.contains("inphic") && lower.contains("kp")) {
        return Some(VendorFingerprint {
            display_name: "Inphic KP 8K",
            manufacturer: "英菲克 Inphic",
            category: "mouse",
            profile_id: "inphic_beken_8k",
            protocol_support: "native-battery-experimental-tuning",
            protocol_note: "已识别英菲克 KP 8K (VID 0x1D57:0xFA65)；已锁定 0xFF00 私有控制通道读取无线电量，DPI/回报率采用 Beken 私有报文试验下发（不同固件版本可能需配合官方驱动校准）。",
            max_polling_hz: 8000,
        });
    }

    // 2. 凌豹 K98PRO 5.0 (VID 0x258A / PID 0x0040 — Sinowealth 中颖电子方案)
    if (vid == 0x258A && pid == 0x0040)
        || lower.contains("k98pro")
        || lower.contains("k98 pro")
        || lower.contains("leobog")
    {
        return Some(VendorFingerprint {
            display_name: "凌豹 K98PRO 5.0",
            manufacturer: "凌豹 / Sinowealth",
            category: "keyboard",
            profile_id: "sinowealth_k98pro",
            protocol_support: "native-battery-readonly",
            protocol_note: "已识别凌豹 K98PRO 5.0 (中颖 0x258A:0x0040)；电量通过 Windows 原生蓝牙属性/只读 HID 实时获取。为防止误写中颖私有 Flash 分区导致键盘锁死，且蓝牙 HOGP 通道不支持烧录，改键与灯效当前保持只读安全模式。",
            max_polling_hz: 1000,
        });
    }

    // 3. 漫步者 HECATE G5 Max (四模声学耳机)
    if lower.contains("g5 max")
        || lower.contains("g5max")
        || (lower.contains("hecate") && lower.contains("g5"))
        || (lower.contains("edifier") && lower.contains("g5"))
    {
        return Some(VendorFingerprint {
            display_name: "漫步者 HECATE G5 Max",
            manufacturer: "漫步者 Edifier HECATE",
            category: "headset",
            profile_id: "edifier_g5_max",
            protocol_support: "system-apo",
            protocol_note: "已识别漫步者 HECATE G5 Max；蓝牙模式已通过 Windows PnP 原生读取实时电量。其板载 DSP EQ 与 THX 空间音效属闭源私有加密协议，建议使用本页「导出 EqualizerAPO 配置」在 Windows 全局无损生效 10 段 EQ。",
            max_polling_hz: 1000,
        });
    }

    // Generic Edifier / HECATE headsets
    if lower.contains("hecate") || lower.contains("edifier") || vid == 0x2D99 {
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
        // Still reject Realtek/system keywords if somehow mixed in
        if !n.contains("realtek") && !n.contains("高清音频") {
            return false;
        }
    }

    // Hard blocklist for system audio, motherboard devices, touchpads, hubs, and virtual drivers
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

    // Block internal I2C / ACPI / SMBus paths
    if p.contains("acpi#") || p.contains("hid#elan") || p.contains("hid#syn") || p.contains("hid#msft") {
        return true;
    }

    // Block standalone non-peripheral HID usages (Digitizer 0x0D, Sensor 0x20, Power/UPS 0x84/0x85, System Control 0x01:0x80)
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
    if name_lower.contains("2.4g")
        || name_lower.contains("receiver")
        || name_lower.contains("dongle")
        || name_lower.contains("wireless")
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
        // Composite gaming mice often expose BOTH Mouse (0x01:0x02) AND Macro Keyboard (0x01:0x06).
        // Prefer mouse if both exist unless name indicates keyboard.
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
}

/// Safe, short-timeout (40ms) vendor HID battery probe.
/// Never blocks the UI thread and clamps/validates returned bytes to 1..=100%.
pub fn probe_hid_vendor_battery(
    api: &HidApi,
    control_path: &str,
    profile_id: &str,
) -> Option<u8> {
    let c_path = CString::new(control_path).ok()?;
    let dev = api.open_path(c_path.as_c_str()).ok()?;

    match profile_id {
        "inphic_beken_8k" => {
            // Beken / CompX 8K wireless mouse battery query (Report ID 0x08 / Cmd 0x04)
            let mut req = [0u8; 17];
            req[0] = 0x08;
            req[1] = 0x04;
            req[16] = 0x49; // Beken standard battery query checksum byte (0x55 - 0x08 - 0x04)
            let _ = dev.write(&req);
            let mut resp = [0u8; 64];
            if let Ok(len) = dev.read_timeout(&mut resp, 40) {
                if len >= 8 {
                    // CompX / Beken replies place battery percentage at byte 6 or byte 7 (0..=100)
                    for &idx in &[6usize, 7usize, 5usize] {
                        if idx < len && resp[idx] >= 1 && resp[idx] <= 100 {
                            return Some(resp[idx]);
                        }
                    }
                }
            }
            // Fallback: read-only feature report probe on Report ID 0x04
            let mut feat = [0u8; 17];
            feat[0] = 0x04;
            if let Ok(len) = dev.get_feature_report(&mut feat) {
                if len >= 6 && feat[5] >= 1 && feat[5] <= 100 {
                    return Some(feat[5]);
                }
            }
            None
        }
        "sinowealth_k98pro" | "edifier_g5_max" | "edifier_generic" => {
            // Strictly READ-ONLY feature report probe (never writes Flash/EEPROM pages)
            for report_id in [0x04u8, 0x05u8, 0x06u8] {
                let mut feat = [0u8; 32];
                feat[0] = report_id;
                if let Ok(len) = dev.get_feature_report(&mut feat) {
                    if len >= 4 {
                        // Look for plausible single-byte battery percentage only if framed cleanly
                        let candidate = feat[3];
                        if candidate >= 1 && candidate <= 100 && feat[1] != 0xFF {
                            return Some(candidate);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Builds a Beken / CompX 8K compatible 17-byte HID output packet for Inphic KP 8K experimental tuning.
pub fn build_inphic_beken_polling_packet(polling_hz: u16) -> [u8; 17] {
    let mut pkt = [0u8; 17];
    pkt[0] = 0x08; // Report ID
    pkt[1] = 0x07; // Write register command
    pkt[4] = 0x00; // Polling rate register offset
    pkt[5] = 0x02; // 2 bytes payload (value + complement)
    let code: u8 = match polling_hz {
        125 => 0x08,
        250 => 0x04,
        500 => 0x02,
        1000 => 0x01,
        2000 => 0x10,
        4000 => 0x20,
        8000 => 0x40,
        _ => 0x01,
    };
    pkt[6] = code;
    pkt[7] = 0x55u8.wrapping_sub(code);
    let sum: u8 = pkt[..16].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    pkt[16] = 0x55u8.wrapping_sub(sum);
    pkt
}

/// Aggregates all raw HID interfaces into deduplicated physical peripherals, fuses Windows
/// Bluetooth PnP battery telemetry, and appends Bluetooth headsets (e.g., Edifier HECATE G5 Max)
/// that expose audio/battery without an HID interface.
pub fn aggregate_real_peripherals(
    api: &HidApi,
    bt_devices: &[WinBluetoothDeviceInfo],
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

        // Filter system / virtual / touchpad / motherboard endpoints
        if is_system_or_virtual_endpoint(&raw_name, &raw_mfr, &path_str, Some(up), Some(u)) {
            continue;
        }

        // Ignore generic empty-name non-mouse/non-keyboard HID nodes unless recognized VID
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
            protocol: infer_protocol_from_path(&path_str, &raw_name),
            has_mouse_usage: false,
            has_keyboard_usage: false,
            has_audio_usage: false,
        });

        // Update best descriptive name if a richer string appears on another interface
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
        // Prefer vendor-defined HID page (>= 0xFF00) for control & battery telemetry
        if up >= 0xFF00 && entry.vendor_control_path.is_none() {
            entry.vendor_control_path = Some(path_str.clone());
        }
        if infer_protocol_from_path(&path_str, &entry.best_name) == "bluetooth" {
            entry.protocol = "bluetooth".to_string();
        }
    }

    let mut results: Vec<DiscoveredPeripheral> = Vec::new();
    let mut matched_bt_names: Vec<String> = Vec::new();

    for ((vid, pid), acc) in grouped {
        // Drop orphaned consumer-control-only nodes that are neither mouse/keyboard nor known brands
        let fp = lookup_vendor_fingerprint(vid, pid, &acc.best_name);
        if fp.is_none() && !acc.has_mouse_usage && !acc.has_keyboard_usage {
            continue;
        }

        let display_name = fp
            .map(|f| f.display_name.to_string())
            .unwrap_or_else(|| acc.best_name.clone());
        let manufacturer = fp
            .map(|f| f.manufacturer.to_string())
            .unwrap_or_else(|| acc.best_manufacturer.clone());
        let category = acc.resolve_category(fp);
        let profile_id = fp.map(|f| f.profile_id).unwrap_or("generic_hid");

        // 1) Try Windows native Bluetooth battery match by (VID, PID) or name
        let bt_match = bt_devices.iter().find(|b| {
            let vid_pid_match = b.vendor_id == Some(vid) && b.product_id == Some(pid);
            let name_lower = b.name.to_ascii_lowercase();
            let disp_lower = display_name.to_ascii_lowercase();
            let fuzzy_name = (name_lower.contains("k98") && disp_lower.contains("k98"))
                || (name_lower.contains("inphic") && disp_lower.contains("inphic"))
                || (name_lower.contains("g5 max") && disp_lower.contains("g5 max"));
            (vid_pid_match || fuzzy_name) && b.battery_percent.is_some()
        });

        let (battery_percent, battery_source) = if let Some(bm) = bt_match {
            matched_bt_names.push(bm.name.to_ascii_lowercase());
            (bm.battery_percent, "windows-bt".to_string())
        } else if let Some(ref ctrl_path) = acc.vendor_control_path {
            if let Some(pct) = probe_hid_vendor_battery(api, ctrl_path, profile_id) {
                (Some(pct), "hid-vendor".to_string())
            } else if acc.protocol == "usb" {
                (None, "wired".to_string())
            } else {
                (None, "unavailable".to_string())
            }
        } else if acc.protocol == "usb" {
            (None, "wired".to_string())
        } else {
            (None, "unavailable".to_string())
        };

        let protocol_support = fp
            .map(|f| f.protocol_support)
            .unwrap_or("native-battery-readonly")
            .to_string();
        let protocol_note = fp
            .map(|f| f.protocol_note.to_string())
            .unwrap_or_else(|| {
                "已通过 Windows 原生 HID 识别；通用外设默认启用安全只读与本地便携配置保存。".to_string()
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
            protocol: acc.protocol,
            category,
            battery_percent,
            battery_source,
            protocol_support,
            protocol_note,
            max_polling_hz,
        });
    }

    // 2) Append pure Bluetooth peripherals (especially Bluetooth headsets like 漫步者 HECATE G5 Max)
    //    that expose DEVPKEY_Bluetooth_Battery in Windows PnP without creating a standard HID device.
    let mut seen_bt_display: Vec<String> = results
        .iter()
        .map(|r| r.product_name.to_ascii_lowercase())
        .collect();

    for bt in bt_devices {
        // Must have either a real battery reading or match a known external peripheral brand
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
        let bt_lower = bt.name.to_ascii_lowercase();
        if matched_bt_names.iter().any(|m| m == &bt_lower) {
            continue;
        }

        let display_name = fp
            .map(|f| f.display_name.to_string())
            .unwrap_or_else(|| bt.name.clone());
        let disp_key = display_name.to_ascii_lowercase();
        if seen_bt_display.iter().any(|s| s == &disp_key) {
            continue;
        }
        seen_bt_display.push(disp_key);

        let category = fp
            .map(|f| f.category.to_string())
            .unwrap_or_else(|| {
                if bt_lower.contains("mouse") || bt_lower.contains("鼠标") {
                    "mouse".to_string()
                } else if bt_lower.contains("key") || bt_lower.contains("kbd") || bt_lower.contains("键盘") {
                    "keyboard".to_string()
                } else {
                    "headset".to_string()
                }
            });

        let vid_str = bt
            .vendor_id
            .map(|v| format!("0x{:04X}", v))
            .unwrap_or_else(|| "BT-PNP".to_string());
        let pid_str = bt
            .product_id
            .map(|p| format!("0x{:04X}", p))
            .unwrap_or_else(|| "BT-AUDIO".to_string());

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
            protocol: "bluetooth".to_string(),
            category,
            battery_percent: bt.battery_percent,
            battery_source: if bt.battery_percent.is_some() {
                "windows-bt".to_string()
            } else {
                "unavailable".to_string()
            },
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

    // Stable deterministic order: mouse -> keyboard -> headset
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
    }

    #[test]
    fn identifies_all_three_user_peripherals() {
        let inphic = lookup_vendor_fingerprint(0x1D57, 0xFA65, "Inphic KP 8K").unwrap();
        assert_eq!(inphic.category, "mouse");
        assert_eq!(inphic.max_polling_hz, 8000);

        let k98 = lookup_vendor_fingerprint(0x258A, 0x0040, "1.0.0").unwrap();
        assert_eq!(k98.display_name, "凌豹 K98PRO 5.0");
        assert_eq!(k98.category, "keyboard");

        let g5 = lookup_vendor_fingerprint(0x0000, 0x0000, "HECATE G5 Max").unwrap();
        assert_eq!(g5.display_name, "漫步者 HECATE G5 Max");
        assert_eq!(g5.category, "headset");
    }
}
