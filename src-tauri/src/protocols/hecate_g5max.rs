//! Edifier HECATE G5 Max HID status/battery/firmware protocol (read-only telemetry in Step 1).
//! Verified against HECATE.exe (`Connecter_G5Max::GetParam` / `TryReadDeviceParam`) and live hardware.

use hidapi::HidApi;
use std::ffi::CString;

pub const VID_HECATE: u16 = 0x35BB;
pub const PID_G5MAX_WIRELESS: u16 = 0xA00B;
pub const PID_G5MAX_WIRED: u16 = 0xA211;
pub const REPORT_ID_WIRELESS: u8 = 0xD0;
pub const REPORT_ID_WIRED: u8 = 0xE0;

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
}
