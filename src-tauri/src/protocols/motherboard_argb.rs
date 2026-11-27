//! Multi-vendor motherboard chassis ARGB lighting controllers with strict byte-level guards.
//! Supports:
//! - ASUS Aura Sync USB (`0x0B05`: `0x18F3` / `0x1939` / `0x19AF` / `0x1AA6` / `0x1BED`, 65B `0xEC` HID)
//! - Gigabyte RGB Fusion 2.0 USB (`0x048D` / `0x0414`, 64B `0xCC` Feature Report)
//! - ASRock Polychrome USB (`0x26CE`: `0x01A2` / `0x01A6`, 65B `0x10` / `0x14` HID)
//! - MSI Mystic Light USB (`0x1462` / `0x0DB0`, detected read-only to prevent MCU bricking)

use crate::safety::HardwareSafetyGuard;
use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use std::ffi::CString;

pub const VID_ASUS: u16 = 0x0B05;
pub const VID_ITE_GIGABYTE: u16 = 0x048D;
pub const VID_GC_GIGABYTE: u16 = 0x0414;
pub const VID_ASROCK: u16 = 0x26CE;
pub const VID_MSI_PRIMARY: u16 = 0x1462;
pub const VID_MSI_COMMON: u16 = 0x0DB0;

pub const ASUS_AURA_REPORT_ID: u8 = 0xEC;
pub const ASUS_CMD_FIRMWARE: u8 = 0x82;
pub const ASUS_CMD_CONFIG_TABLE: u8 = 0xB0;
pub const ASUS_CMD_EFFECT: u8 = 0x35;
pub const ASUS_CMD_EFFECT_COLOR: u8 = 0x36;
pub const ASUS_CMD_COMMIT: u8 = 0x3F;
pub const ASUS_CMD_DIRECT: u8 = 0x40;
pub const ASUS_CMD_GEN1_MODE: u8 = 0x52;
pub const ASUS_LEDS_PER_PACKET: usize = 20;
pub const ASUS_DIRECT_APPLY_FLAG: u8 = 0x80;
pub const ASUS_DIRECT_OVERFLOW_FLAG: u8 = 0x10;

pub const GIGABYTE_REPORT_ID: u8 = 0xCC;
pub const ASROCK_CMD_SET_ZONE: u8 = 0x10;
pub const ASROCK_CMD_COMMIT: u8 = 0x12;
pub const ASROCK_CMD_READ_CFG: u8 = 0x14;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgbHeaderInfo {
    pub index: u8,
    pub effect_channel: u8,
    pub direct_channel: u8,
    pub name: String,
    pub header_type: String,
    pub max_leds: u16,
    pub default_leds: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisRgbCapabilities {
    pub available: bool,
    pub vendor: String,
    pub protocol: String,
    pub motherboard_name: String,
    pub controller_name: String,
    pub firmware_version: Option<String>,
    pub vendor_id: String,
    pub product_id: String,
    pub hid_path: Option<String>,
    pub write_supported: bool,
    pub conflict_service_running: Option<String>,
    pub headers: Vec<ArgbHeaderInfo>,
    pub supported_modes: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderLightingZoneConfig {
    pub index: u8,
    pub label: String,
    pub enabled: bool,
    pub color: String,
    pub led_count: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisLightingConfig {
    pub preset_id: String,
    pub mode: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub brightness: u8,
    pub speed: u8,
    pub per_header_custom: bool,
    pub headers: Vec<HeaderLightingZoneConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisLightingDispatchResult {
    pub dispatched_to_hardware: bool,
    pub packet_count: usize,
    pub preview_bytes: Vec<u8>,
    pub detail: String,
}

fn empty_capabilities(mb_name: String) -> ChassisRgbCapabilities {
    ChassisRgbCapabilities {
        available: false,
        vendor: "None".to_string(),
        protocol: "none".to_string(),
        motherboard_name: mb_name,
        controller_name: "未检测到主板 ARGB 控制器".to_string(),
        firmware_version: None,
        vendor_id: "0x0000".to_string(),
        product_id: "0x0000".to_string(),
        hid_path: None,
        write_supported: false,
        conflict_service_running: None,
        headers: Vec::new(),
        supported_modes: Vec::new(),
        note: "当前主机未检测到受支持的主板 USB HID ARGB 控制器".to_string(),
    }
}

pub fn parse_hex_rgb(hex: &str) -> [u8; 3] {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&trimmed[0..2], 16),
            u8::from_str_radix(&trimmed[2..4], 16),
            u8::from_str_radix(&trimmed[4..6], 16),
        ) {
            return [r, g, b];
        }
    }
    [0x2B, 0x6B, 0x68]
}

pub fn scale_rgb(rgb: [u8; 3], brightness: u8) -> [u8; 3] {
    let pct = brightness.min(100) as u16;
    [
        ((rgb[0] as u16 * pct) / 100) as u8,
        ((rgb[1] as u16 * pct) / 100) as u8,
        ((rgb[2] as u16 * pct) / 100) as u8,
    ]
}

pub fn generate_gradient_strip(
    start: [u8; 3],
    end: [u8; 3],
    count: usize,
    brightness: u8,
) -> Vec<[u8; 3]> {
    let n = count.clamp(1, 240);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = if n <= 1 {
            0.0
        } else {
            i as f32 / (n - 1) as f32
        };
        let r = (start[0] as f32 + (end[0] as f32 - start[0] as f32) * t).round() as u8;
        let g = (start[1] as f32 + (end[1] as f32 - start[1] as f32) * t).round() as u8;
        let b = (start[2] as f32 + (end[2] as f32 - start[2] as f32) * t).round() as u8;
        out.push(scale_rgb([r, g, b], brightness));
    }
    out
}

pub fn make_default_lighting_config(caps: &ChassisRgbCapabilities) -> ChassisLightingConfig {
    let default_labels = [
        "ARGB 针脚 1（水冷 / 主区）",
        "ARGB 针脚 2（机箱风扇组 A）",
        "ARGB 针脚 3（机箱风扇组 B）",
        "主板板载区域",
    ];
    let headers = if caps.headers.is_empty() {
        vec![HeaderLightingZoneConfig {
            index: 0,
            label: default_labels[0].to_string(),
            enabled: true,
            color: "#2B6B68".to_string(),
            led_count: 30,
        }]
    } else {
        caps.headers
            .iter()
            .enumerate()
            .map(|(i, hdr)| HeaderLightingZoneConfig {
                index: hdr.index,
                label: if hdr.header_type == "onboard" {
                    hdr.name.clone()
                } else {
                    default_labels
                        .get(i)
                        .map(|s| (*s).to_string())
                        .unwrap_or_else(|| hdr.name.clone())
                },
                enabled: true,
                color: match i % 3 {
                    0 => "#2B6B68".to_string(),
                    1 => "#38BDF8".to_string(),
                    _ => "#818CF8".to_string(),
                },
                led_count: hdr.default_leds.clamp(1, hdr.max_leds.max(1)),
            })
            .collect()
    };

    ChassisLightingConfig {
        preset_id: "default-rainbow".to_string(),
        mode: "rainbow".to_string(),
        primary_color: "#2B6B68".to_string(),
        secondary_color: "#38BDF8".to_string(),
        brightness: 85,
        speed: 50,
        per_header_custom: false,
        headers,
    }
}

/// Reads motherboard manufacturer + product from Windows DMI Registry (`HKLM\HARDWARE\DESCRIPTION\System\BIOS`).
#[cfg(windows)]
pub fn read_motherboard_dmi_name() -> String {
    use core::ffi::c_void;

    const HKEY_LOCAL_MACHINE: *mut c_void = 0x8000_0002usize as *mut c_void;
    const KEY_READ: u32 = 0x20019;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            h_key: *mut c_void,
            lp_sub_key: *const u16,
            ul_options: u32,
            sam_desired: u32,
            phk_result: *mut *mut c_void,
        ) -> i32;
        fn RegQueryValueExW(
            h_key: *mut c_void,
            lp_value_name: *const u16,
            lp_reserved: *mut u32,
            lp_type: *mut u32,
            lp_data: *mut u8,
            lpcb_data: *mut u32,
        ) -> i32;
        fn RegCloseKey(h_key: *mut c_void) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn read_reg_string(h_key: *mut c_void, name: &str) -> Option<String> {
        let wname = to_wide(name);
        let mut size: u32 = 0;
        let rc = unsafe {
            RegQueryValueExW(
                h_key,
                wname.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if rc != 0 || size < 2 || size > 4096 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let rc2 = unsafe {
            RegQueryValueExW(
                h_key,
                wname.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut size,
            )
        };
        if rc2 != 0 {
            return None;
        }
        let u16s: Vec<u16> = buf[..size as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let end = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
        let s = String::from_utf16_lossy(&u16s[..end]).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    let subkey = to_wide(r"HARDWARE\DESCRIPTION\System\BIOS");
    let mut h_key: *mut c_void = std::ptr::null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &mut h_key,
        )
    };
    if opened != 0 || h_key.is_null() {
        return "主板 ARGB 控制器".to_string();
    }

    let mfg = read_reg_string(h_key, "BaseBoardManufacturer");
    let prod = read_reg_string(h_key, "BaseBoardProduct");
    unsafe {
        RegCloseKey(h_key);
    }

    match (mfg, prod) {
        (Some(m), Some(p)) => {
            if p.to_ascii_uppercase().contains("TUF")
                || p.to_ascii_uppercase().contains("ROG")
                || p.to_ascii_uppercase().contains("PRIME")
                || p.to_ascii_uppercase().contains("MAG")
                || p.to_ascii_uppercase().contains("MPG")
                || p.to_ascii_uppercase().contains("MEG")
                || p.to_ascii_uppercase().contains("AORUS")
            {
                p
            } else {
                format!("{m} {p}")
            }
        }
        (None, Some(p)) => p,
        (Some(m), None) => m,
        (None, None) => "主板 ARGB 控制器".to_string(),
    }
}

#[cfg(not(windows))]
pub fn read_motherboard_dmi_name() -> String {
    "主板 ARGB 控制器".to_string()
}

/// Detects OEM motherboard lighting background services (Aura LightingService, Armoury Crate, RGB Fusion, Polychrome).
#[cfg(windows)]
pub fn detect_conflicting_rgb_process() -> Option<String> {
    use core::ffi::c_void;

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const INVALID_HANDLE: *mut c_void = (-1isize) as *mut c_void;

    #[repr(C)]
    struct PROCESSENTRY32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
        fn Process32FirstW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
        fn Process32NextW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
        fn CloseHandle(h_object: *mut c_void) -> i32;
    }

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == INVALID_HANDLE {
            return None;
        }

        let mut entry = PROCESSENTRY32W {
            dw_size: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            cnt_usage: 0,
            th32_process_id: 0,
            th32_default_heap_id: 0,
            th32_module_id: 0,
            cnt_threads: 0,
            th32_parent_process_id: 0,
            pc_pri_class_base: 0,
            dw_flags: 0,
            sz_exe_file: [0u16; 260],
        };

        let mut detected: Option<String> = None;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let nul_pos = entry
                    .sz_exe_file
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.sz_exe_file.len());
                let exe = String::from_utf16_lossy(&entry.sz_exe_file[..nul_pos]);
                let lower = exe.to_ascii_lowercase();
                if lower == "lightingservice.exe" {
                    detected = Some("ASUS Aura LightingService.exe".to_string());
                    break;
                } else if lower == "armourycrate.service.exe" || lower == "armourycrate.exe" {
                    detected = Some("ASUS Armoury Crate".to_string());
                    break;
                } else if lower == "rgbfusion.exe" {
                    detected = Some("Gigabyte RGBFusion.exe".to_string());
                    break;
                } else if lower == "asrrgbled.exe" {
                    detected = Some("ASRock Polychrome ASRRGBLED.exe".to_string());
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        detected
    }
}

#[cfg(not(windows))]
pub fn detect_conflicting_rgb_process() -> Option<String> {
    None
}

pub fn asus_mode_byte(mode: &str) -> Option<u8> {
    match mode {
        "off" => Some(0x00),
        "static" => Some(0x01),
        "breathing" => Some(0x02),
        "flashing" => Some(0x03),
        "spectrum" => Some(0x04),
        "rainbow" => Some(0x05),
        "rainbow-breathing" => Some(0x06),
        "chase-fade" => Some(0x07),
        "rainbow-chase-fade" => Some(0x08),
        "chase" => Some(0x09),
        "rainbow-chase" => Some(0x0A),
        "wave" => Some(0x0B),
        "chase-rainbow-pulse" => Some(0x0C),
        "random-flicker" => Some(0x0D),
        "direct-gradient" | "direct-per-header" => Some(0xFF),
        _ => None,
    }
}

pub fn asus_mode_uses_color(mode_byte: u8) -> bool {
    matches!(mode_byte, 0x01 | 0x02 | 0x03 | 0x07 | 0x09)
}

/// Parses OpenRGB-verified ASUS Aura 60-byte config table into structured ARGB/onboard zones.
pub fn parse_asus_config_table(fw: &str, cfg_table: &[u8]) -> Vec<ArgbHeaderInfo> {
    if cfg_table.len() < 32 {
        return Vec::new();
    }
    let num_argb = (cfg_table[0x02] as usize).min(6);
    let mut num_onboard = cfg_table[0x1B];
    let num_rgb_headers = cfg_table[0x1D];
    if num_onboard < num_rgb_headers {
        num_onboard = 0;
    }

    let argb_from_zero = fw.starts_with("AULA3-AR32");
    let mut out = Vec::new();
    let mut effect_channel: u8 = 0;

    if num_onboard > 0 {
        let onboard_eff_ch = if argb_from_zero {
            num_argb as u8
        } else {
            let ch = effect_channel;
            effect_channel += 1;
            ch
        };
        out.push(ArgbHeaderInfo {
            index: 0,
            effect_channel: onboard_eff_ch,
            direct_channel: 0x04,
            name: "主板板载灯光区".to_string(),
            header_type: "onboard".to_string(),
            max_leds: num_onboard as u16,
            default_leds: num_onboard as u16,
        });
    }

    for i in 0..num_argb {
        let idx = out.len() as u8;
        let entry_off = 3 + i * 6;
        let hw_max = cfg_table
            .get(entry_off + 3)
            .copied()
            .filter(|&v| (1..=240).contains(&v))
            .unwrap_or(120) as u16;
        out.push(ArgbHeaderInfo {
            index: idx,
            effect_channel,
            direct_channel: i as u8,
            name: format!("Gen2 5V ARGB 针脚 {}", i + 1),
            header_type: "argb_gen2".to_string(),
            max_leds: hw_max,
            default_leds: 30.min(hw_max),
        });
        effect_channel = effect_channel.saturating_add(1);
    }

    out
}

pub fn build_asus_query_packet(subcmd: u8) -> [u8; 65] {
    let mut pkt = [0u8; 65];
    pkt[0] = ASUS_AURA_REPORT_ID;
    pkt[1] = subcmd;
    pkt
}

pub fn build_asus_gen1_mode_packet() -> [u8; 65] {
    let mut pkt = [0u8; 65];
    pkt[0] = ASUS_AURA_REPORT_ID;
    pkt[1] = ASUS_CMD_GEN1_MODE;
    pkt[2] = 0x53;
    pkt[3] = 0x00;
    pkt[4] = 0x01;
    pkt
}

pub fn build_asus_effect_packet(effect_channel: u8, mode_byte: u8) -> [u8; 65] {
    let mut pkt = [0u8; 65];
    pkt[0] = ASUS_AURA_REPORT_ID;
    pkt[1] = ASUS_CMD_EFFECT;
    pkt[2] = effect_channel;
    pkt[3] = 0x00;
    pkt[4] = 0x00;
    pkt[5] = mode_byte;
    pkt
}

pub fn build_asus_effect_color_packet(
    start_led: u8,
    led_count: u8,
    rgb: [u8; 3],
) -> [u8; 65] {
    let count = led_count.clamp(1, 16);
    let mask: u16 = (((1u32 << count) - 1) << start_led.min(15)) as u16;
    let mut pkt = [0u8; 65];
    pkt[0] = ASUS_AURA_REPORT_ID;
    pkt[1] = ASUS_CMD_EFFECT_COLOR;
    pkt[2] = (mask >> 8) as u8;
    pkt[3] = (mask & 0xFF) as u8;
    pkt[4] = 0x00;
    for i in 0..count as usize {
        let base = 5 + (start_led as usize + i) * 3;
        if base + 2 < 65 {
            pkt[base] = rgb[0];
            pkt[base + 1] = rgb[1];
            pkt[base + 2] = rgb[2];
        }
    }
    pkt
}

pub fn build_asus_commit_packet() -> [u8; 65] {
    let mut pkt = [0u8; 65];
    pkt[0] = ASUS_AURA_REPORT_ID;
    pkt[1] = ASUS_CMD_COMMIT;
    pkt[2] = 0x55;
    pkt
}

pub fn build_asus_direct_packets(direct_channel: u8, colors: &[[u8; 3]]) -> Vec<[u8; 65]> {
    if colors.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let total = colors.len();
    let mut configured = 0usize;
    let mut leds_offset = 0u8;
    let mut cmd = direct_channel;

    while configured < total {
        let remaining = total - configured;
        let in_pkt = if remaining > ASUS_LEDS_PER_PACKET {
            ASUS_LEDS_PER_PACKET
        } else {
            cmd |= ASUS_DIRECT_APPLY_FLAG;
            remaining
        };

        let mut pkt = [0u8; 65];
        pkt[0] = ASUS_AURA_REPORT_ID;
        pkt[1] = ASUS_CMD_DIRECT;
        pkt[2] = cmd;
        pkt[3] = leds_offset;
        pkt[4] = in_pkt as u8;

        for idx in 0..in_pkt {
            let rgb = colors[configured + idx];
            pkt[5 + idx * 3] = rgb[0];
            pkt[6 + idx * 3] = rgb[1];
            pkt[7 + idx * 3] = rgb[2];
        }
        out.push(pkt);

        configured += in_pkt;
        leds_offset = leds_offset.wrapping_add(in_pkt as u8);
        if configured > 255 {
            cmd |= ASUS_DIRECT_OVERFLOW_FLAG;
            leds_offset = (configured - 256) as u8;
        }
    }
    out
}

/// Strict byte-level pre-flight validator guarding every 65B ASUS Aura HID write.
pub fn validate_asus_aura_frame(pkt: &[u8; 65]) -> Result<(), String> {
    if pkt[0] != ASUS_AURA_REPORT_ID {
        return Err(format!(
            "安全护栏拦截：ASUS Aura 报文 Report ID 非法 (0x{:02X}，预期 0xEC)",
            pkt[0]
        ));
    }
    match pkt[1] {
        ASUS_CMD_FIRMWARE | ASUS_CMD_CONFIG_TABLE => {
            if !pkt[2..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：ASUS 查询报文尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        ASUS_CMD_GEN1_MODE => {
            if pkt[2..5] != [0x53, 0x00, 0x01] || !pkt[5..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：ASUS Gen1 模式指令格式校验失败".to_string());
            }
            Ok(())
        }
        ASUS_CMD_EFFECT => {
            if pkt[5] > 0x13 && pkt[5] != 0xFF {
                return Err(format!(
                    "安全护栏拦截：ASUS Aura 灯效模式 byte 0x{:02X} 越界",
                    pkt[5]
                ));
            }
            if !pkt[6..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：ASUS 灯效指令尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        ASUS_CMD_EFFECT_COLOR => Ok(()),
        ASUS_CMD_DIRECT => {
            let count = pkt[4] as usize;
            if count == 0 || count > ASUS_LEDS_PER_PACKET {
                return Err(format!(
                    "安全护栏拦截：ASUS Direct 单包灯珠数 {count} 越界（允许 1..=20）"
                ));
            }
            let used_end = 5 + count * 3;
            if !pkt[used_end..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：ASUS Direct 报文未用尾部字节非零".to_string());
            }
            Ok(())
        }
        ASUS_CMD_COMMIT => {
            if pkt[2] != 0x55 || !pkt[3..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：ASUS Commit 指令校验失败".to_string());
            }
            Ok(())
        }
        other => Err(format!(
            "安全护栏拦截：未知的 ASUS Aura 子命令 0x{other:02X}"
        )),
    }
}

pub fn gigabyte_mode_byte(mode: &str) -> Option<u8> {
    match mode {
        "off" => Some(0),
        "static" | "direct-gradient" | "direct-per-header" => Some(1),
        "breathing" => Some(2),
        "flashing" => Some(3),
        "spectrum" => Some(4),
        "rainbow" | "wave" => Some(6),
        "random-flicker" => Some(8),
        "chase" | "chase-fade" => Some(9),
        _ => None,
    }
}

pub fn build_gigabyte_effect_packet(
    zone_mask: u32,
    mode_byte: u8,
    rgb: [u8; 3],
    brightness: u8,
    speed: u8,
) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = GIGABYTE_REPORT_ID;
    pkt[1] = 0x20;
    pkt[2..6].copy_from_slice(&zone_mask.to_le_bytes());
    pkt[11] = mode_byte;
    pkt[12] = brightness.clamp(1, 100);
    pkt[13] = if mode_byte == 6 || mode_byte == 4 { 7 } else { 0 };
    // Gigabyte uses BGR color order inside effect packet bytes 18..=20
    pkt[18] = rgb[2];
    pkt[19] = rgb[1];
    pkt[20] = rgb[0];
    let period = 400u16.saturating_add((100u16.saturating_sub(speed.min(100) as u16)) * 10);
    pkt[26..28].copy_from_slice(&period.to_le_bytes());
    pkt[28..30].copy_from_slice(&period.to_le_bytes());
    pkt
}

pub fn build_gigabyte_apply_packet() -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = GIGABYTE_REPORT_ID;
    pkt[1] = 0x28;
    pkt[2] = 0xFF;
    pkt
}

pub fn validate_gigabyte_frame(pkt: &[u8; 64]) -> Result<(), String> {
    if pkt[0] != GIGABYTE_REPORT_ID {
        return Err(format!(
            "安全护栏拦截：Gigabyte RGB Fusion 2 Report ID 非法 (0x{:02X})",
            pkt[0]
        ));
    }
    if !(0x20..=0x28).contains(&pkt[1]) {
        return Err(format!(
            "安全护栏拦截：未授权的 Gigabyte 指令码 0x{:02X}",
            pkt[1]
        ));
    }
    Ok(())
}

pub fn asrock_mode_byte(mode: &str) -> Option<u8> {
    match mode {
        "off" => Some(0x00),
        "static" | "direct-gradient" | "direct-per-header" => Some(0x01),
        "breathing" => Some(0x02),
        "flashing" => Some(0x03),
        "spectrum" => Some(0x04),
        "random-flicker" => Some(0x05),
        "wave" => Some(0x07),
        "chase" | "chase-fade" => Some(0x0B),
        "rainbow" => Some(0x0E),
        _ => None,
    }
}

pub fn build_asrock_write_zone_packet(
    zone_type: u8,
    mode_byte: u8,
    speed: u8,
    rgb: [u8; 3],
    all_zones: bool,
) -> [u8; 65] {
    let mut pkt = [0u8; 65];
    pkt[0] = 0x00;
    pkt[1] = ASROCK_CMD_SET_ZONE;
    pkt[3] = zone_type;
    pkt[4] = mode_byte;
    // Standard ASRock GRB order
    pkt[5] = rgb[1];
    pkt[6] = rgb[0];
    pkt[7] = rgb[2];
    pkt[8] = 255u8.saturating_sub(((speed.min(100) as u16 * 255) / 100) as u8);
    pkt[9] = 0xFF;
    pkt[16] = u8::from(all_zones);
    pkt
}

pub fn validate_asrock_frame(pkt: &[u8; 65]) -> Result<(), String> {
    if pkt[0] != 0x00 {
        return Err("安全护栏拦截：ASRock 报文 Report ID 必须为 0x00".to_string());
    }
    if !matches!(pkt[1], ASROCK_CMD_SET_ZONE | ASROCK_CMD_COMMIT | ASROCK_CMD_READ_CFG) {
        return Err(format!(
            "安全护栏拦截：未授权的 ASRock 命令码 0x{:02X}",
            pkt[1]
        ));
    }
    if pkt[1] == ASROCK_CMD_SET_ZONE && pkt[4] > 0x0E {
        return Err(format!(
            "安全护栏拦截：ASRock 模式 byte 0x{:02X} 越界",
            pkt[4]
        ));
    }
    Ok(())
}

fn is_supported_asus_mb_pid(pid: u16) -> bool {
    matches!(pid, 0x18F3 | 0x1939 | 0x19AF | 0x1AA6 | 0x1BED)
}

fn is_supported_gigabyte_mb(vid: u16, pid: u16) -> bool {
    (vid == VID_ITE_GIGABYTE && matches!(pid, 0x8297 | 0x8950 | 0x5702 | 0x5711))
        || (vid == VID_GC_GIGABYTE && pid == 0xA100)
}

fn is_supported_asrock_mb(vid: u16, pid: u16) -> bool {
    vid == VID_ASROCK && matches!(pid, 0x01A2 | 0x01A6)
}

fn is_msi_mystic_mb(vid: u16, pid: u16) -> bool {
    (vid == VID_MSI_PRIMARY || vid == VID_MSI_COMMON) && (pid >= 0x7000 || pid == 0x0076)
}

fn probe_asus_controller(
    api: &HidApi,
    vid: u16,
    pid: u16,
    path: &str,
    mb_name: String,
    conflict: Option<String>,
) -> Option<ChassisRgbCapabilities> {
    let c_path = CString::new(path).ok()?;
    let dev = api.open_path(c_path.as_c_str()).ok()?;

    let mut resp = [0u8; 65];
    let fw_req = build_asus_query_packet(ASUS_CMD_FIRMWARE);
    let fw_str = if dev.write(&fw_req).is_ok()
        && dev.read_timeout(&mut resp, 250).unwrap_or(0) >= 18
        && resp[1] == 0x02
    {
        let end = resp[2..18].iter().position(|&b| b == 0).unwrap_or(16);
        Some(String::from_utf8_lossy(&resp[2..2 + end]).trim().to_string())
    } else {
        None
    };

    let mut cfg_resp = [0u8; 65];
    let cfg_req = build_asus_query_packet(ASUS_CMD_CONFIG_TABLE);
    let headers = if dev.write(&cfg_req).is_ok()
        && dev.read_timeout(&mut cfg_resp, 250).unwrap_or(0) >= 36
        && cfg_resp[1] == 0x30
    {
        parse_asus_config_table(fw_str.as_deref().unwrap_or(""), &cfg_resp[4..64])
    } else {
        return None;
    };

    if headers.is_empty() {
        return None;
    }

    let argb_cnt = headers
        .iter()
        .filter(|h| h.header_type == "argb_gen2")
        .count();

    Some(ChassisRgbCapabilities {
        available: true,
        vendor: "ASUS".to_string(),
        protocol: "asus_aura_usb".to_string(),
        motherboard_name: mb_name,
        controller_name: format!("ASUS Aura Sync USB ({argb_cnt}× Gen2 ARGB)"),
        firmware_version: fw_str,
        vendor_id: format!("0x{vid:04X}"),
        product_id: format!("0x{pid:04X}"),
        hid_path: Some(path.to_string()),
        write_supported: true,
        conflict_service_running: conflict,
        headers,
        supported_modes: vec![
            "rainbow",
            "wave",
            "spectrum",
            "static",
            "breathing",
            "chase-fade",
            "direct-gradient",
            "direct-per-header",
            "off",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        note: "已通过 ASUS Aura USB 0xEC 只读握手机制发现主板 ARGB 接口；调节仅写 RAM，未触发高危 Flash 操作。".to_string(),
    })
}

fn probe_gigabyte_controller(
    api: &HidApi,
    vid: u16,
    pid: u16,
    path: &str,
    mb_name: String,
    conflict: Option<String>,
) -> Option<ChassisRgbCapabilities> {
    let c_path = CString::new(path).ok()?;
    let dev = api.open_path(c_path.as_c_str()).ok()?;
    let mut buf = [0u8; 64];
    buf[0] = GIGABYTE_REPORT_ID;
    let len = dev.get_feature_report(&mut buf).ok()?;
    if len < 40 {
        return None;
    }

    let fw = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
    let argb_count = ((buf[8] & 0x0F).max(2)).min(4);
    let mut headers = Vec::new();
    for i in 0..argb_count {
        headers.push(ArgbHeaderInfo {
            index: i,
            effect_channel: i,
            direct_channel: i,
            name: format!("D_LED {} (5V ARGB)", i + 1),
            header_type: "argb_gen2".to_string(),
            max_leds: 256,
            default_leds: 30,
        });
    }

    Some(ChassisRgbCapabilities {
        available: true,
        vendor: "Gigabyte".to_string(),
        protocol: "gigabyte_fusion2_usb".to_string(),
        motherboard_name: mb_name,
        controller_name: "Gigabyte RGB Fusion 2.0 (IT8297/IT57xx)".to_string(),
        firmware_version: Some(fw),
        vendor_id: format!("0x{vid:04X}"),
        product_id: format!("0x{pid:04X}"),
        hid_path: Some(path.to_string()),
        write_supported: true,
        conflict_service_running: conflict,
        headers,
        supported_modes: vec![
            "rainbow",
            "spectrum",
            "static",
            "breathing",
            "flashing",
            "chase",
            "off",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        note: "已识别技嘉 IT8297/IT57xx USB HID 控制器，支持硬件级预设灯效与一键恢复默认。".to_string(),
    })
}

fn probe_asrock_controller(
    vid: u16,
    pid: u16,
    path: &str,
    mb_name: String,
    conflict: Option<String>,
) -> ChassisRgbCapabilities {
    let headers = vec![
        ArgbHeaderInfo {
            index: 0,
            effect_channel: 0x02,
            direct_channel: 0x02,
            name: "ADDR_LED 1 (5V ARGB)".to_string(),
            header_type: "argb_gen2".to_string(),
            max_leds: 100,
            default_leds: 30,
        },
        ArgbHeaderInfo {
            index: 1,
            effect_channel: 0x03,
            direct_channel: 0x03,
            name: "ADDR_LED 2 (5V ARGB)".to_string(),
            header_type: "argb_gen2".to_string(),
            max_leds: 100,
            default_leds: 30,
        },
    ];

    ChassisRgbCapabilities {
        available: true,
        vendor: "ASRock".to_string(),
        protocol: "asrock_polychrome_usb".to_string(),
        motherboard_name: mb_name,
        controller_name: "ASRock Polychrome USB".to_string(),
        firmware_version: None,
        vendor_id: format!("0x{vid:04X}"),
        product_id: format!("0x{pid:04X}"),
        hid_path: Some(path.to_string()),
        write_supported: true,
        conflict_service_running: conflict,
        headers,
        supported_modes: vec![
            "rainbow",
            "wave",
            "spectrum",
            "static",
            "breathing",
            "chase",
            "off",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        note: "已识别华擎 Polychrome USB 控制器（0x26CE），支持 5V ARGB 针脚硬件灯效下发。".to_string(),
    }
}

/// Strictly read-only hardware probe for motherboard chassis ARGB controllers.
pub fn probe_chassis_rgb_capabilities(api: &HidApi) -> ChassisRgbCapabilities {
    let mb_name = read_motherboard_dmi_name();
    let conflict = detect_conflicting_rgb_process();

    // 1. ASUS Aura Sync USB (prefer UsagePage 0xFF72 / Usage 0x00A1 when present)
    let mut asus_candidates: Vec<(u16, String, u16, u16)> = Vec::new();
    let mut gigabyte_candidates: Vec<(u16, u16, String)> = Vec::new();
    let mut asrock_candidates: Vec<(u16, u16, String)> = Vec::new();
    let mut msi_candidate: Option<(u16, u16, String)> = None;

    for info in api.device_list() {
        let vid = info.vendor_id();
        let pid = info.product_id();
        let upg = info.usage_page();
        let usage = info.usage();
        let path = info.path().to_string_lossy().to_string();

        if vid == VID_ASUS && is_supported_asus_mb_pid(pid) {
            asus_candidates.push((pid, path, upg, usage));
        } else if is_supported_gigabyte_mb(vid, pid) && (upg == 0xFF89 || upg == 0xFF00) {
            gigabyte_candidates.push((vid, pid, path));
        } else if is_supported_asrock_mb(vid, pid) {
            asrock_candidates.push((vid, pid, path));
        } else if msi_candidate.is_none() && is_msi_mystic_mb(vid, pid) {
            msi_candidate = Some((vid, pid, path));
        }
    }

    // Prioritize exact Aura HID interface (0xFF72 / 0x00A1)
    asus_candidates.sort_by_key(|(_, _, upg, usage)| {
        if *upg == 0xFF72 && *usage == 0x00A1 {
            0
        } else {
            1
        }
    });

    for (pid, path, _, _) in asus_candidates {
        if let Some(caps) =
            probe_asus_controller(api, VID_ASUS, pid, &path, mb_name.clone(), conflict.clone())
        {
            return caps;
        }
    }

    for (vid, pid, path) in gigabyte_candidates {
        if let Some(caps) =
            probe_gigabyte_controller(api, vid, pid, &path, mb_name.clone(), conflict.clone())
        {
            return caps;
        }
    }

    if let Some((vid, pid, path)) = asrock_candidates.into_iter().next() {
        return probe_asrock_controller(vid, pid, &path, mb_name, conflict);
    }

    if let Some((vid, pid, path)) = msi_candidate {
        return ChassisRgbCapabilities {
            available: true,
            vendor: "MSI".to_string(),
            protocol: "msi_mystic_readonly".to_string(),
            motherboard_name: mb_name,
            controller_name: "MSI Mystic Light USB（防变砖只读保护）".to_string(),
            firmware_version: None,
            vendor_id: format!("0x{vid:04X}"),
            product_id: format!("0x{pid:04X}"),
            hid_path: Some(path),
            write_supported: false,
            conflict_service_running: conflict,
            headers: vec![ArgbHeaderInfo {
                index: 0,
                effect_channel: 0,
                direct_channel: 0,
                name: "JRAINBOW ARGB（只读探测）".to_string(),
                header_type: "argb_gen2".to_string(),
                max_leds: 120,
                default_leds: 30,
            }],
            supported_modes: vec!["rainbow".to_string()],
            note: "微星部分 Mystic Light 芯片对 HID 包长极其敏感（历史曾有 184/185 字节不匹配变砖案例），当前已锁定为只读保护模式；如需调灯请使用 MSI Center。".to_string(),
        };
    }

    empty_capabilities(mb_name)
}

pub fn apply_chassis_rgb_config(
    api: &HidApi,
    caps: &ChassisRgbCapabilities,
    cfg: &ChassisLightingConfig,
) -> Result<ChassisLightingDispatchResult, String> {
    if !caps.available {
        return Err("未检测到可用主板 ARGB 控制器，仅保存本地预设配置".to_string());
    }
    if !caps.write_supported {
        return Err(caps.note.clone());
    }

    let vid = u16::from_str_radix(caps.vendor_id.trim_start_matches("0x"), 16).unwrap_or(0);
    let pid = u16::from_str_radix(caps.product_id.trim_start_matches("0x"), 16).unwrap_or(0);
    HardwareSafetyGuard::check_write_allowed(vid, pid, "motherboard_argb_apply")?;

    let path = caps
        .hid_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "缺少主板 ARGB HID 设备路径".to_string())?;

    let primary = scale_rgb(parse_hex_rgb(&cfg.primary_color), cfg.brightness);
    let secondary = scale_rgb(parse_hex_rgb(&cfg.secondary_color), cfg.brightness);

    match caps.protocol.as_str() {
        "asus_aura_usb" => {
            let mode_byte = asus_mode_byte(&cfg.mode).ok_or_else(|| {
                format!("安全护栏拦截：不支持的 ASUS Aura 灯效模式 {}", cfg.mode)
            })?;
            let mut frames: Vec<[u8; 65]> = Vec::new();

            if mode_byte == 0xFF {
                frames.push(build_asus_gen1_mode_packet());
                for hdr in &caps.headers {
                    frames.push(build_asus_effect_packet(hdr.effect_channel, 0xFF));
                    let zone_cfg = cfg.headers.iter().find(|h| h.index == hdr.index);
                    let enabled = zone_cfg.map(|z| z.enabled).unwrap_or(true);
                    let count = zone_cfg
                        .map(|z| z.led_count.clamp(1, hdr.max_leds.max(1)))
                        .unwrap_or(hdr.default_leds) as usize;
                    let strip_colors = if !enabled {
                        vec![[0u8, 0u8, 0u8]; count]
                    } else if cfg.mode == "direct-per-header" || cfg.per_header_custom {
                        let zone_rgb = zone_cfg
                            .map(|z| scale_rgb(parse_hex_rgb(&z.color), cfg.brightness))
                            .unwrap_or(primary);
                        vec![zone_rgb; count]
                    } else {
                        generate_gradient_strip(primary, secondary, count, 100)
                    };
                    frames.extend(build_asus_direct_packets(hdr.direct_channel, &strip_colors));
                }
            } else {
                let mut start_led: u8 = 0;
                for hdr in &caps.headers {
                    let zone_cfg = cfg.headers.iter().find(|h| h.index == hdr.index);
                    let enabled = zone_cfg.map(|z| z.enabled).unwrap_or(true);
                    let eff_mode = if enabled { mode_byte } else { 0x00 };
                    frames.push(build_asus_effect_packet(hdr.effect_channel, eff_mode));
                    if enabled && asus_mode_uses_color(eff_mode) {
                        let rgb = if cfg.per_header_custom {
                            zone_cfg
                                .map(|z| scale_rgb(parse_hex_rgb(&z.color), cfg.brightness))
                                .unwrap_or(primary)
                        } else {
                            primary
                        };
                        let led_slot_count = if hdr.header_type == "onboard" {
                            (hdr.max_leds as u8).clamp(1, 8)
                        } else {
                            1
                        };
                        frames.push(build_asus_effect_color_packet(
                            start_led,
                            led_slot_count,
                            rgb,
                        ));
                    }
                    start_led = start_led.saturating_add(1);
                }
            }

            for pkt in &frames {
                validate_asus_aura_frame(pkt)?;
            }

            let c_path =
                CString::new(path).map_err(|_| "主板 ARGB HID 路径包含非法空字符".to_string())?;
            let dev = api
                .open_path(c_path.as_c_str())
                .map_err(|e| format!("打开 ASUS Aura HID 接口失败: {e}"))?;
            for (idx, pkt) in frames.iter().enumerate() {
                if idx > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(4));
                }
                dev.write(pkt)
                    .map_err(|e| format!("写入 ASUS Aura 灯光报文失败: {e}"))?;
            }

            let preview = frames.first().map(|f| f[..16].to_vec()).unwrap_or_default();
            Ok(ChassisLightingDispatchResult {
                dispatched_to_hardware: true,
                packet_count: frames.len(),
                preview_bytes: preview,
                detail: format!(
                    "已同步 {} 至 {}（{} 帧 0xEC 校验通过{}）",
                    cfg.mode,
                    caps.motherboard_name,
                    frames.len(),
                    caps.conflict_service_running
                        .as_ref()
                        .map(|s| format!("；提示：{s} 正在运行，若灯效被覆盖可在需要时暂停该服务"))
                        .unwrap_or_default()
                ),
            })
        }
        "gigabyte_fusion2_usb" => {
            let mode_byte = gigabyte_mode_byte(&cfg.mode).ok_or_else(|| {
                format!("安全护栏拦截：Gigabyte 不支持的灯效模式 {}", cfg.mode)
            })?;
            let zone_mask: u32 = cfg
                .headers
                .iter()
                .filter(|h| h.enabled)
                .fold(0u32, |acc, h| acc | (1u32 << h.index.min(7)));
            let eff_pkt = build_gigabyte_effect_packet(
                if zone_mask == 0 { 0xFF } else { zone_mask },
                mode_byte,
                primary,
                cfg.brightness,
                cfg.speed,
            );
            let apply_pkt = build_gigabyte_apply_packet();
            validate_gigabyte_frame(&eff_pkt)?;
            validate_gigabyte_frame(&apply_pkt)?;

            let c_path =
                CString::new(path).map_err(|_| "主板 ARGB HID 路径包含非法空字符".to_string())?;
            let dev = api
                .open_path(c_path.as_c_str())
                .map_err(|e| format!("打开 Gigabyte RGB Fusion 接口失败: {e}"))?;
            dev.send_feature_report(&eff_pkt)
                .map_err(|e| format!("下发 Gigabyte 灯效 Feature Report 失败: {e}"))?;
            dev.send_feature_report(&apply_pkt)
                .map_err(|e| format!("提交 Gigabyte 灯效失败: {e}"))?;

            Ok(ChassisLightingDispatchResult {
                dispatched_to_hardware: true,
                packet_count: 2,
                preview_bytes: eff_pkt[..20].to_vec(),
                detail: format!("已同步 {} 至技嘉 RGB Fusion 2.0", cfg.mode),
            })
        }
        "asrock_polychrome_usb" => {
            let mode_byte = asrock_mode_byte(&cfg.mode).ok_or_else(|| {
                format!("安全护栏拦截：ASRock 不支持的灯效模式 {}", cfg.mode)
            })?;
            let pkt = build_asrock_write_zone_packet(0x02, mode_byte, cfg.speed, primary, true);
            validate_asrock_frame(&pkt)?;

            let c_path =
                CString::new(path).map_err(|_| "主板 ARGB HID 路径包含非法空字符".to_string())?;
            let dev = api
                .open_path(c_path.as_c_str())
                .map_err(|e| format!("打开 ASRock Polychrome 接口失败: {e}"))?;
            dev.write(&pkt)
                .map_err(|e| format!("写入 ASRock Polychrome 灯效失败: {e}"))?;

            Ok(ChassisLightingDispatchResult {
                dispatched_to_hardware: true,
                packet_count: 1,
                preview_bytes: pkt[..18].to_vec(),
                detail: format!("已同步 {} 至华擎 Polychrome USB", cfg.mode),
            })
        }
        other => Err(format!("不支持的主板协议类型: {other}")),
    }
}

/// Resets motherboard ARGB hardware back to safe factory rainbow effect & commits state cleanly.
pub fn reset_chassis_rgb_to_default(
    api: &HidApi,
    caps: &ChassisRgbCapabilities,
) -> Result<(ChassisLightingConfig, ChassisLightingDispatchResult), String> {
    let default_cfg = make_default_lighting_config(caps);
    let mut dispatch = apply_chassis_rgb_config(api, caps, &default_cfg)?;

    if caps.protocol == "asus_aura_usb" {
        if let Some(path) = caps.hid_path.as_deref() {
            if let Ok(c_path) = CString::new(path) {
                if let Ok(dev) = api.open_path(c_path.as_c_str()) {
                    let commit_pkt = build_asus_commit_packet();
                    if validate_asus_aura_frame(&commit_pkt).is_ok() {
                        let _ = dev.write(&commit_pkt);
                        dispatch.packet_count += 1;
                    }
                }
            }
        }
    }
    dispatch.detail = format!(
        "已一键恢复 {} 出厂默认彩虹流光（已退出 Direct 接管并释放固件原生灯效）",
        caps.motherboard_name
    );
    Ok((default_cfg, dispatch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_tuf_b850m_e_wifi_config_table() {
        // Real live capture from TUF GAMING B850M-E WIFI (`0x0B05:0x19AF`, FW `AULA3-AR32-0309`)
        let mut ct = [0u8; 60];
        let prefix: [u8; 32] = [
            0x1E, 0x9F, 0x03, 0x01, 0x00, 0x00, 0x78, 0x3C, 0x00, 0x01, 0x00, 0x00, 0x78, 0x3C,
            0x00, 0x01, 0x00, 0x00, 0x78, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x03, 0x03, 0x01, 0xF4,
        ];
        ct[..prefix.len()].copy_from_slice(&prefix);

        let headers = parse_asus_config_table("AULA3-AR32-0309", &ct);
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].header_type, "argb_gen2");
        assert_eq!(headers[0].effect_channel, 0);
        assert_eq!(headers[0].direct_channel, 0);
        assert_eq!(headers[0].max_leds, 120);
        assert_eq!(headers[1].effect_channel, 1);
        assert_eq!(headers[2].effect_channel, 2);
        assert_eq!(headers[2].direct_channel, 2);
    }

    #[test]
    fn builds_and_validates_asus_direct_chunks_and_blocks_invalid_frames() {
        let strip = generate_gradient_strip([0x2B, 0x6B, 0x68], [0x38, 0xBD, 0xF8], 35, 100);
        assert_eq!(strip.len(), 35);

        let packets = build_asus_direct_packets(1, &strip);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0][0], 0xEC);
        assert_eq!(packets[0][1], 0x40);
        assert_eq!(packets[0][2], 0x01); // channel 1, continue
        assert_eq!(packets[0][4], 20);
        assert_eq!(packets[1][2], 0x81); // channel 1 | AURA_DIRECT_APPLY (0x80)
        assert_eq!(packets[1][3], 20);
        assert_eq!(packets[1][4], 15);

        for pkt in &packets {
            assert!(validate_asus_aura_frame(pkt).is_ok());
        }

        // Reject invalid frames
        let mut wrong_id = build_asus_effect_packet(0, 0x05);
        wrong_id[0] = 0xEE;
        assert!(validate_asus_aura_frame(&wrong_id).is_err());

        let mut bad_subcmd = build_asus_effect_packet(0, 0x05);
        bad_subcmd[1] = 0x99;
        assert!(validate_asus_aura_frame(&bad_subcmd).is_err());

        let mut bad_mode = build_asus_effect_packet(0, 0x20);
        bad_mode[5] = 0x20;
        assert!(validate_asus_aura_frame(&bad_mode).is_err());

        let mut over_leds = packets[0];
        over_leds[4] = 21;
        assert!(validate_asus_aura_frame(&over_leds).is_err());
    }

    #[test]
    fn validates_gigabyte_and_asrock_frames() {
        let g_eff = build_gigabyte_effect_packet(0x07, 6, [0x2B, 0x6B, 0x68], 80, 50);
        assert_eq!(g_eff[0], 0xCC);
        assert_eq!(g_eff[1], 0x20);
        assert!(validate_gigabyte_frame(&g_eff).is_ok());

        let mut g_bad = g_eff;
        g_bad[1] = 0x9F;
        assert!(validate_gigabyte_frame(&g_bad).is_err());

        let a_pkt = build_asrock_write_zone_packet(0x02, 0x0E, 60, [0x2B, 0x6B, 0x68], true);
        assert!(validate_asrock_frame(&a_pkt).is_ok());

        let mut a_bad = a_pkt;
        a_bad[4] = 0x55;
        assert!(validate_asrock_frame(&a_bad).is_err());
    }

    #[test]
    #[ignore = "Read-only live motherboard ARGB capability probe"]
    fn live_readonly_probe_motherboard_argb() {
        let api = HidApi::new().expect("HidApi init failed");
        let caps = probe_chassis_rgb_capabilities(&api);
        println!("Live Motherboard ARGB Capabilities: {caps:#?}");
        assert!(caps.available, "Expected live motherboard ARGB controller");
        assert_eq!(caps.vendor, "ASUS");
        assert_eq!(caps.headers.len(), 3);
    }
}
