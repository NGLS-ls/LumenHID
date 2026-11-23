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
    pkt[4] = 0x00;
    pkt
}

/// Builds a 64-byte LED mode write frame (`Report ID 0xED`: `[ED, 35, BB, 02, 02, mode, 0...]`).
/// Modes supported by HECATE G5 Max: 0 = Off, 1 = Static, 2 = Breathing, 3 = Rainbow Cycle.
pub fn build_led_mode_packet(mode: u8) -> [u8; 64] {
    let mut pkt = [0u8; 64];
    pkt[0] = REPORT_ID_LED;
    pkt[1] = 0x35;
    pkt[2] = 0xBB;
    pkt[3] = 0x02;
    pkt[4] = 0x02;
    pkt[5] = mode.min(4);
    pkt
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
    mode: u8,
) -> Result<[u8; 64], String> {
    validate_led_write_preconditions(vid, pid, led_path)?;
    let pkt = build_led_mode_packet(mode);
    let c_path = CString::new(led_path).map_err(|_| "LED HID 路径包含非法空字符".to_string())?;
    let dev = api
        .open_path(c_path.as_c_str())
        .map_err(|e| format!("打开 HECATE LED 接口失败: {e}"))?;
    dev.write(&pkt)
        .map_err(|e| format!("写入 HECATE LED 灯效指令失败: {e}"))?;
    Ok(pkt)
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
    fn builds_led_packets_and_enforces_path_safety() {
        let q = build_led_query_packet();
        assert_eq!(&q[..6], &[0xED, 0x35, 0xBB, 0x01, 0x00, 0x00]);
        assert_eq!(q.len(), 64);

        let m = build_led_mode_packet(3);
        assert_eq!(&m[..6], &[0xED, 0x35, 0xBB, 0x02, 0x02, 0x03]);
        assert_eq!(build_led_mode_packet(9)[5], 4);

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
