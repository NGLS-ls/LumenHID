//! Edifier HECATE G5 Max HID status/battery/firmware protocol (read-only telemetry in Step 1).
//! Verified against HECATE.exe (`Connecter_G5Max::GetParam` / `TryReadDeviceParam`) and live hardware.

use crate::safety::HardwareSafetyGuard;
use hidapi::HidApi;
use std::ffi::CString;

pub const VID_HECATE: u16 = 0x35BB;
pub const PID_G5MAX_WIRELESS: u16 = 0xA00B;
pub const PID_G5MAX_WIRED: u16 = 0xA211;
pub const REPORT_ID_WIRELESS: u8 = 0xD0;
pub const REPORT_ID_WIRED: u8 = 0xE0;
pub const REPORT_ID_LED: u8 = 0xED;

pub const LED_SUBCMD_QUERY: u8 = 0x00;
pub const LED_SUBCMD_POWER: u8 = 0x02;
pub const LED_SUBCMD_MODE: u8 = 0x03;
pub const LED_SUBCMD_SPEED: u8 = 0x04;
pub const LED_SUBCMD_BRIGHTNESS: u8 = 0x05;
pub const LED_SUBCMD_COLOR: u8 = 0x07;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HecateLedEffectMode {
    Constant = 0x00,
    Breathe = 0x01,
    RainbowCycle = 0x02,
}

impl HecateLedEffectMode {
    pub fn from_hw_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Constant),
            0x01 => Some(Self::Breathe),
            0x02 => Some(Self::RainbowCycle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct G5MaxLedStatus {
    pub led_on: bool,
    pub mode: HecateLedEffectMode,
    pub speed: u8,
    pub brightness: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G5MaxStatus {
    pub online: bool,
    pub charging: bool,
    pub battery_percent: Option<u8>,
    pub device_fw: String,
    pub dongle_fw: String,
}

/// Builds a 64-byte HECATE G5 Max HID query frame: `[ReportID, 0x35, 0xBB, 0x01, SubCmd, 0...]`.
pub fn build_status_query_packet(is_wireless: bool, subcmd: u8) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = if is_wireless {
        REPORT_ID_WIRELESS
    } else {
        REPORT_ID_WIRED
    };
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x01;
    pkt[4] = subcmd;
    pkt
}

/// Parses a 64-byte `SubCmd=0x00` status response from HECATE G5 Max.
pub fn parse_status_response(resp: &[u8], expected_report_id: u8) -> Option<G5MaxStatus> {
    if resp.len() < 14 {
        return None;
    }
    if resp[0] != expected_report_id || resp[1] != 0x35 || resp[2] != 0xBB || resp[4] != 0x00 {
        return None;
    }

    let flags = resp[5];
    let online = (flags & 0x01) != 0;
    let charging = (flags & 0x04) != 0 || (resp[6] & 0x80) != 0;

    let battery_percent = if online {
        let pct = resp[6] & 0x7F;
        if (1..=100).contains(&pct) {
            Some(pct)
        } else {
            None
        }
    } else {
        None
    };

    let device_fw = format!("{}.{}.{}", resp[8], resp[9], resp[10]);
    let dongle_fw = format!("{}.{}.{}", resp[11], resp[12], resp[13]);

    Some(G5MaxStatus {
        online,
        charging,
        battery_percent,
        device_fw,
        dongle_fw,
    })
}

/// Queries HECATE G5 Max real-time battery, charging state, and firmware version over `mi_03&col03`
/// (2.4G) or `mi_03&col02` (USB wired). Only sends a read-only status query (`SubCmd = 0x00`).
pub fn query_g5max_status(
    api: &HidApi,
    control_path: &str,
    is_wireless: bool,
) -> Option<G5MaxStatus> {
    let c_path = CString::new(control_path).ok()?;
    let dev = api.open_path(c_path.as_c_str()).ok()?;
    let req = build_status_query_packet(is_wireless, 0x00);
    if dev.write(&req).is_err() {
        return None;
    }

    let expected_rid = req[0];
    let mut resp = [0u8; 64];
    for _ in 0..3 {
        match dev.read_timeout(&mut resp, 250) {
            Ok(len) if len >= 14 => {
                if let Some(status) = parse_status_response(&resp[..len], expected_rid) {
                    return Some(status);
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    None
}

/// Builds a 64-byte read-only LED status query frame (`Report ID 0xED`).
#[allow(dead_code)]
pub fn build_led_query_packet() -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x01;
    pkt[4] = LED_SUBCMD_QUERY;
    pkt
}

/// Parses a 64-byte `0xED` LED status response (`ED 35 BB 06 00 <LedOn> <Mode> <Speed> <Bright>`).
#[allow(dead_code)]
pub fn parse_led_status_response(resp: &[u8]) -> Option<G5MaxLedStatus> {
    if resp.len() < 9 {
        return None;
    }
    if resp[0] != REPORT_ID_LED
        || resp[1] != 0x35
        || resp[2] != 0xBB
        || resp[3] != 0x06
        || resp[4] != LED_SUBCMD_QUERY
    {
        return None;
    }
    let led_on = match resp[5] {
        0x00 => false,
        0x01 => true,
        _ => return None,
    };
    let mode = HecateLedEffectMode::from_hw_byte(resp[6])?;
    let speed = resp[7];
    let brightness = resp[8];
    if !(1..=10).contains(&speed) || !(1..=10).contains(&brightness) {
        return None;
    }
    Some(G5MaxLedStatus {
        led_on,
        mode,
        speed,
        brightness,
    })
}

/// Builds a 64-byte LED power switch frame (`ED 35 BB 02 02 <0|1>`, verified at `HECATE.exe 0x14030531e`).
/// Strictly restricts byte 5 to `0x00` (off) or `0x01` (on) — never writes effect mode indices into `0x02`.
pub fn build_led_power_packet(on: bool) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x02;
    pkt[4] = LED_SUBCMD_POWER;
    pkt[5] = u8::from(on);
    pkt
}

/// Builds a 64-byte LED effect mode frame (`ED 35 BB 02 03 <0|1|2>`, verified at `HECATE.exe 0x14030559a`).
pub fn build_led_effect_mode_packet(mode: HecateLedEffectMode) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x02;
    pkt[4] = LED_SUBCMD_MODE;
    pkt[5] = mode as u8;
    pkt
}

/// Builds a 64-byte LED speed frame (`ED 35 BB 02 04 <1..=10>`, verified at `HECATE.exe 0x140305802`).
#[allow(dead_code)]
pub fn build_led_speed_packet(speed: u8) -> Result<[u8; 64], String> {
    if !(1..=10).contains(&speed) {
        return Err(format!(
            "安全护栏拦截：HECATE LED 速度参数 {speed} 越界（仅允许 1..=10）"
        ));
    }
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x02;
    pkt[4] = LED_SUBCMD_SPEED;
    pkt[5] = speed;
    Ok(pkt)
}

/// Builds a 64-byte LED brightness frame (`ED 35 BB 02 05 <1..=10>`, verified at `HECATE.exe 0x140305a4e`).
#[allow(dead_code)]
pub fn build_led_brightness_packet(brightness: u8) -> Result<[u8; 64], String> {
    if !(1..=10).contains(&brightness) {
        return Err(format!(
            "安全护栏拦截：HECATE LED 亮度参数 {brightness} 越界（仅允许 1..=10）"
        ));
    }
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x02;
    pkt[4] = LED_SUBCMD_BRIGHTNESS;
    pkt[5] = brightness;
    Ok(pkt)
}

/// Builds a 64-byte LED RGB color frame (`ED 35 BB 04 07 <R> <G> <B>`, verified at `HECATE.exe 0x140305c9a`).
#[allow(dead_code)]
pub fn build_led_color_packet(r: u8, g: u8, b: u8) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x04;
    pkt[4] = LED_SUBCMD_COLOR;
    pkt[5] = r;
    pkt[6] = g;
    pkt[7] = b;
    pkt
}

/// Maps UI LED mode indices (`0=关闭, 1=常亮, 2=呼吸, 3=幻彩循环`) to a validated `0xED` HID sequence.
pub fn build_led_ui_mode_sequence(ui_mode: u8) -> Result<Vec<[u8; 64]>, String> {
    match ui_mode {
        0 => Ok(vec![build_led_power_packet(false)]),
        1 => Ok(vec![
            build_led_power_packet(true),
            build_led_effect_mode_packet(HecateLedEffectMode::Constant),
        ]),
        2 => Ok(vec![
            build_led_power_packet(true),
            build_led_effect_mode_packet(HecateLedEffectMode::Breathe),
        ]),
        3 => Ok(vec![
            build_led_power_packet(true),
            build_led_effect_mode_packet(HecateLedEffectMode::RainbowCycle),
        ]),
        _ => Err(format!(
            "安全护栏拦截：不支持的 HECATE 灯效模式编号 {ui_mode}（仅允许 0..=3）"
        )),
    }
}

/// Strict byte-level pre-flight validator guarding every `0xED` HID write before `dev.write()`.
pub fn validate_led_packet_frame(pkt: &[u8; 64]) -> Result<(), String> {
    if pkt[0] != REPORT_ID_LED || pkt[1] != 0x35 || pkt[2] != 0xBB {
        return Err(format!(
            "安全护栏拦截：HECATE LED 报文头非法 ({:02X} {:02X} {:02X})",
            pkt[0], pkt[1], pkt[2]
        ));
    }

    match (pkt[3], pkt[4]) {
        (0x01, LED_SUBCMD_QUERY) => {
            if !pkt[5..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 查询帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (0x02, LED_SUBCMD_POWER) => {
            if pkt[5] > 0x01 {
                return Err(format!(
                    "安全护栏拦截：LED 开关子命令 0x02 越界（收到 0x{:02X}，仅允许 0x00 或 0x01）",
                    pkt[5]
                ));
            }
            if !pkt[6..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 开关帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (0x02, LED_SUBCMD_MODE) => {
            if pkt[5] > 0x02 {
                return Err(format!(
                    "安全护栏拦截：LED 模式子命令 0x03 越界（收到 0x{:02X}，仅允许 0x00..=0x02）",
                    pkt[5]
                ));
            }
            if !pkt[6..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 模式帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (0x02, LED_SUBCMD_SPEED) => {
            if !(1..=10).contains(&pkt[5]) {
                return Err(format!(
                    "安全护栏拦截：LED 速度子命令 0x04 越界（收到 {}，仅允许 1..=10）",
                    pkt[5]
                ));
            }
            if !pkt[6..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 速度帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (0x02, LED_SUBCMD_BRIGHTNESS) => {
            if !(1..=10).contains(&pkt[5]) {
                return Err(format!(
                    "安全护栏拦截：LED 亮度子命令 0x05 越界（收到 {}，仅允许 1..=10）",
                    pkt[5]
                ));
            }
            if !pkt[6..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 亮度帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (0x04, LED_SUBCMD_COLOR) => {
            if !pkt[8..].iter().all(|&b| b == 0) {
                return Err("安全护栏拦截：LED 颜色帧尾部存在非零残留字节".to_string());
            }
            Ok(())
        }
        (len, sub) => Err(format!(
            "安全护栏拦截：未知或未授权的 HECATE LED 子命令 (len=0x{len:02X}, subcmd=0x{sub:02X})"
        )),
    }
}

/// Checks whether the official `HECATE.exe` driver is running on Windows so LumenHID never
/// performs concurrent vendor HID writes on the same endpoint.
#[cfg(windows)]
pub fn is_hecate_official_driver_running() -> bool {
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
            return false;
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

        let mut found = false;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let nul_pos = entry
                    .sz_exe_file
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.sz_exe_file.len());
                let exe_name = String::from_utf16_lossy(&entry.sz_exe_file[..nul_pos]);
                if exe_name.eq_ignore_ascii_case("hecate.exe") {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
pub fn is_hecate_official_driver_running() -> bool {
    false
}

/// Validates anti-brick rules + LED collection path (`mi_03&col04` wireless or `mi_03&col03` wired)
/// before writing a 64-byte `0xED` RGB mode packet.
pub fn validate_led_write_preconditions(
    vid: u16,
    pid: u16,
    led_path: &str,
) -> Result<(), String> {
    HardwareSafetyGuard::check_write_allowed(vid, pid, "hecate_led")?;
    let lower = led_path.to_ascii_lowercase();
    if !lower.contains("vid_35bb") || (!lower.contains("col04") && !lower.contains("col03")) {
        return Err(format!(
            "安全护栏拦截：HECATE LED 路径格式不匹配 ({led_path})"
        ));
    }
    Ok(())
}

pub fn apply_g5max_led_mode(
    api: &HidApi,
    led_path: &str,
    vid: u16,
    pid: u16,
    ui_mode: u8,
) -> Result<Vec<[u8; 64]>, String> {
    validate_led_write_preconditions(vid, pid, led_path)?;
    if is_hecate_official_driver_running() {
        return Err(
            "安全护栏拦截：检测到官方驱动 HECATE.exe 正在运行，为避免双驱动并发写入 HID 通道导致耳机固件状态异常，请先退出 HECATE 后再调整灯效"
                .to_string(),
        );
    }

    let packets = build_led_ui_mode_sequence(ui_mode)?;
    for pkt in &packets {
        validate_led_packet_frame(pkt)?;
    }

    let c_path = CString::new(led_path).map_err(|_| "LED HID 路径包含非法空字符".to_string())?;
    let dev = api
        .open_path(c_path.as_c_str())
        .map_err(|e| format!("打开 HECATE LED 接口失败: {e}"))?;

    for (idx, pkt) in packets.iter().enumerate() {
        if idx > 0 {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        dev.write(pkt)
            .map_err(|e| format!("写入 HECATE LED 灯效指令失败: {e}"))?;
    }

    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_wireless_and_wired_query_packets() {
        let w = build_status_query_packet(true, 0x00);
        assert_eq!(&w[..6], &[0xD0, 0x35, 0xBB, 0x01, 0x00, 0x00]);
        assert_eq!(w.len(), 64);

        let u = build_status_query_packet(false, 0x01);
        assert_eq!(&u[..6], &[0xE0, 0x35, 0xBB, 0x01, 0x01, 0x00]);
    }

    #[test]
    fn parses_real_captured_g5max_response() {
        // Real hardware capture: d0 35 bb 0a 00 01 3a 00 01 05 01 02 03 08
        let mut frame = [0u8; 64];
        let prefix = [0xD0, 0x35, 0xBB, 0x0A, 0x00, 0x01, 0x3A, 0x00, 0x01, 0x05, 0x01, 0x02, 0x03, 0x08];
        frame[..prefix.len()].copy_from_slice(&prefix);

        let st = parse_status_response(&frame, REPORT_ID_WIRELESS).unwrap();
        assert!(st.online);
        assert!(!st.charging);
        assert_eq!(st.battery_percent, Some(58));
        assert_eq!(st.device_fw, "1.5.1");
        assert_eq!(st.dongle_fw, "2.3.8");
    }

    #[test]
    fn parses_charging_and_offline_edge_cases() {
        let mut charging_frame = [0u8; 64];
        charging_frame[..14].copy_from_slice(&[
            0xE0, 0x35, 0xBB, 0x0A, 0x00, 0x05, 0xC8, 0x00, 0x01, 0x05, 0x01, 0x00, 0x00, 0x00,
        ]);
        let st = parse_status_response(&charging_frame, REPORT_ID_WIRED).unwrap();
        assert!(st.online);
        assert!(st.charging);
        assert_eq!(st.battery_percent, Some(0xC8 & 0x7F)); // 72%

        let mut offline_frame = charging_frame;
        offline_frame[5] = 0x00;
        let off = parse_status_response(&offline_frame, REPORT_ID_WIRED).unwrap();
        assert!(!off.online);
        assert_eq!(off.battery_percent, None);
    }

    #[test]
    fn builds_and_validates_split_led_protocol_packets() {
        let q = build_led_query_packet();
        assert_eq!(&q[..6], &[0xED, 0x35, 0xBB, 0x01, 0x00, 0x00]);
        assert_eq!(q.len(), 64);
        assert!(validate_led_packet_frame(&q).is_ok());

        let p_off = build_led_power_packet(false);
        assert_eq!(&p_off[..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x00]);
        assert!(validate_led_packet_frame(&p_off).is_ok());

        let p_on = build_led_power_packet(true);
        assert_eq!(&p_on[..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x01]);
        assert!(validate_led_packet_frame(&p_on).is_ok());

        let m_const = build_led_effect_mode_packet(HecateLedEffectMode::Constant);
        let m_breathe = build_led_effect_mode_packet(HecateLedEffectMode::Breathe);
        let m_rainbow = build_led_effect_mode_packet(HecateLedEffectMode::RainbowCycle);
        assert_eq!(&m_const[..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x00]);
        assert_eq!(&m_breathe[..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x01]);
        assert_eq!(&m_rainbow[..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x02]);

        assert_eq!(
            &build_led_speed_packet(1).unwrap()[..6],
            &[0xED, 0x35, 0xBB, 0x02, 0x04, 0x01]
        );
        assert_eq!(
            &build_led_speed_packet(10).unwrap()[..6],
            &[0xED, 0x35, 0xBB, 0x02, 0x04, 0x0A]
        );
        assert!(build_led_speed_packet(0).is_err());
        assert!(build_led_speed_packet(11).is_err());

        assert_eq!(
            &build_led_brightness_packet(1).unwrap()[..6],
            &[0xED, 0x35, 0xBB, 0x02, 0x05, 0x01]
        );
        assert_eq!(
            &build_led_brightness_packet(10).unwrap()[..6],
            &[0xED, 0x35, 0xBB, 0x02, 0x05, 0x0A]
        );
        assert!(build_led_brightness_packet(0).is_err());
        assert!(build_led_brightness_packet(11).is_err());

        let color = build_led_color_packet(0x2B, 0x6B, 0x68);
        assert_eq!(&color[..8], &[0xED, 0x35, 0xBB, 0x04, 0x07, 0x2B, 0x6B, 0x68]);
        assert!(validate_led_packet_frame(&color).is_ok());
    }

    #[test]
    fn builds_ui_mode_sequences_and_rejects_out_of_range_frames() {
        let seq0 = build_led_ui_mode_sequence(0).unwrap();
        assert_eq!(seq0.len(), 1);
        assert_eq!(&seq0[0][..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x00]);

        let seq1 = build_led_ui_mode_sequence(1).unwrap();
        assert_eq!(seq1.len(), 2);
        assert_eq!(&seq1[0][..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x01]);
        assert_eq!(&seq1[1][..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x00]);

        let seq2 = build_led_ui_mode_sequence(2).unwrap();
        assert_eq!(seq2.len(), 2);
        assert_eq!(&seq2[0][..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x01]);
        assert_eq!(&seq2[1][..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x01]);

        let seq3 = build_led_ui_mode_sequence(3).unwrap();
        assert_eq!(seq3.len(), 2);
        assert_eq!(&seq3[0][..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x01]);
        assert_eq!(&seq3[1][..6], &[0xED, 0x35, 0xBB, 0x02, 0x03, 0x02]);

        for seq in [&seq0, &seq1, &seq2, &seq3] {
            for pkt in seq {
                assert!(validate_led_packet_frame(pkt).is_ok());
            }
        }

        assert!(build_led_ui_mode_sequence(4).is_err());
        assert!(build_led_ui_mode_sequence(9).is_err());
        assert!(build_led_ui_mode_sequence(255).is_err());

        // Reject exact historical buggy frames captured in log_20261123.log (`ED 35 BB 02 02 02/03`)
        let mut buggy_power_2 = [0u8; 64];
        buggy_power_2[..6].copy_from_slice(&[0xED, 0x35, 0xBB, 0x02, 0x02, 0x02]);
        assert!(validate_led_packet_frame(&buggy_power_2).is_err());

        let mut buggy_power_3 = [0u8; 64];
        buggy_power_3[..6].copy_from_slice(&[0xED, 0x35, 0xBB, 0x02, 0x02, 0x03]);
        assert!(validate_led_packet_frame(&buggy_power_3).is_err());

        let mut out_of_range_mode = [0u8; 64];
        out_of_range_mode[..6].copy_from_slice(&[0xED, 0x35, 0xBB, 0x02, 0x03, 0x03]);
        assert!(validate_led_packet_frame(&out_of_range_mode).is_err());

        let mut trailing_garbage = build_led_power_packet(true);
        trailing_garbage[6] = 0x01;
        assert!(validate_led_packet_frame(&trailing_garbage).is_err());

        let mut unknown_subcmd = [0u8; 64];
        unknown_subcmd[..6].copy_from_slice(&[0xED, 0x35, 0xBB, 0x02, 0x09, 0x00]);
        assert!(validate_led_packet_frame(&unknown_subcmd).is_err());
    }

    #[test]
    fn parses_real_captured_led_status_response() {
        // Real hardware capture from log_20261123.log: ED 35 BB 06 00 01 02 05 0A
        let mut resp = [0u8; 64];
        resp[..9].copy_from_slice(&[0xED, 0x35, 0xBB, 0x06, 0x00, 0x01, 0x02, 0x05, 0x0A]);
        let st = parse_led_status_response(&resp).unwrap();
        assert!(st.led_on);
        assert_eq!(st.mode, HecateLedEffectMode::RainbowCycle);
        assert_eq!(st.speed, 5);
        assert_eq!(st.brightness, 10);

        let mut bad_mode = resp;
        bad_mode[6] = 0x03;
        assert!(parse_led_status_response(&bad_mode).is_none());

        let mut bad_power = resp;
        bad_power[5] = 0x02;
        assert!(parse_led_status_response(&bad_power).is_none());

        let mut bad_speed = resp;
        bad_speed[7] = 0x00;
        assert!(parse_led_status_response(&bad_speed).is_none());
    }

    #[test]
    fn enforces_led_write_path_safety() {
        assert!(validate_led_write_preconditions(
            VID_HECATE,
            PID_G5MAX_WIRELESS,
            r"\\?\hid#vid_35bb&pid_a00b&mi_03&col04#7&1234"
        )
        .is_ok());
        assert!(validate_led_write_preconditions(
            VID_HECATE,
            PID_G5MAX_WIRELESS,
            r"\\?\hid#vid_35bb&pid_a00b&mi_03&col01#7&1234"
        )
        .is_err());
        assert!(validate_led_write_preconditions(
            0x258A,
            0x019D,
            r"\\?\hid#vid_258a&pid_019d&mi_03&col04#7&1234"
        )
        .is_err());
    }
}
