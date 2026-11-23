//! Inphic IN10 / KP 8K (`0x1D57:0xFA65`) passive COL03 telemetry parser & COL04 64-byte HID writer.
//! Verified against INPHIC.exe (`0x414b70`, `0x44f3a0`, `0x460a00`, `0x46173e`, `0x461de0`, `0x462260`)
//! and Data/ms_3/pro.data (`model_id = 0x93`).

use crate::safety::HardwareSafetyGuard;
use crate::telemetry_daemon;
use hidapi::{HidApi, HidDevice};
use std::ffi::CString;
use std::time::Duration;

pub const VID_INPHIC: u16 = 0x1D57;
pub const PID_KP8K_DONGLE: u16 = 0xFA65;
pub const MODEL_ID_IN10: u8 = 0x93;

/// Strict write opcode allowlist (excludes 0x09 macro flash & unverified commands).
const ALLOWED_WRITE_OPCODES: [u8; 4] = [0x04, 0x06, 0x07, 0x0C];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InphicTelemetryEvent {
    Battery {
        model_id: u8,
        percent: Option<u8>,
        charging: bool,
        online: bool,
    },
    DpiStageChanged {
        model_id: u8,
        stage_index: u8,
    },
    PowerState {
        model_id: u8,
        sleeping: bool,
    },
}

pub fn model_name_from_id(id: u8) -> &'static str {
    match id {
        0x93 => "Inphic IN10 (KP 8K)",
        0x95 => "Inphic IN9 PRO",
        0x90 => "Inphic IN9 Ultra",
        0x99 => "Inphic INX",
        0x98 => "Inphic IN10R",
        0x73 => "Inphic IN10R MAX",
        0x71 => "Inphic IN10 PRO",
        0x74 => "Inphic IN10 MAX",
        0x72 => "Inphic IN10 3D",
        0x77 => "Inphic IN9 PRO V2",
        _ => "Inphic KP 8K",
    }
}

/// Parses a 5-byte HID Input Report received passively from `MI_02&COL03` (`UP=0x000A, U=0x0000`).
pub fn parse_col03_report(buf: &[u8]) -> Option<InphicTelemetryEvent> {
    if buf.len() < 5 || buf[0] != 0x03 {
        return None;
    }
    let model_id = buf[1];
    let event_type = buf[2];

    match event_type {
        0x40 => {
            let status = buf[3];
            let mode = status & 0x0F;
            let upper_nibble = (status >> 4) & 0x0F;
            let charging = mode == 3 || upper_nibble == 1;
            let percent = if mode == 2 {
                Some(100)
            } else if (1..=100).contains(&buf[4]) {
                Some(buf[4])
            } else {
                None
            };
            Some(InphicTelemetryEvent::Battery {
                model_id,
                percent,
                charging,
                online: true,
            })
        }
        0x10 => {
            let raw_stage = buf[3];
            if (1..=8).contains(&raw_stage) {
                Some(InphicTelemetryEvent::DpiStageChanged {
                    model_id,
                    stage_index: raw_stage - 1,
                })
            } else {
                None
            }
        }
        0x50 => Some(InphicTelemetryEvent::PowerState {
            model_id,
            sleeping: buf[3] == 0,
        }),
        _ => None,
    }
}

/// Wraps an inner Inphic `ms_3` command payload inside the 64-byte Report ID `0x04` outer frame:
/// `[0x04, N+5, 0x00, cmd_payload[0..N], sum_hi, sum_lo, 0...]`.
/// Enforces the safe opcode allowlist (explicitly rejects `0x09` macro flash & unknown opcodes).
pub fn wrap_report04_frame(cmd_payload: &[u8]) -> Result<[u8; 64], String> {
    if cmd_payload.is_empty() || cmd_payload.len() > 58 {
        return Err(format!(
            "Inphic 0x04 帧载荷长度越界（要求 1..=58 字节，实际 {}）",
            cmd_payload.len()
        ));
    }
    let opcode = cmd_payload[0];
    if !ALLOWED_WRITE_OPCODES.contains(&opcode) {
        return Err(format!(
            "安全护栏拦截：禁止发送未列入白名单的 Inphic 指令码 0x{opcode:02X}"
        ));
    }

    let n = cmd_payload.len();
    let mut pkt = [0u8; 64];
    pkt[0] = 0x04;
    pkt[1] = (n + 5) as u8;
    pkt[2] = 0x00;
    pkt[3..3 + n].copy_from_slice(cmd_payload);

    let sum = pkt[0..3 + n]
        .iter()
        .fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
    pkt[3 + n] = (sum >> 8) as u8;
    pkt[3 + n + 1] = (sum & 0xFF) as u8;
    Ok(pkt)
}

/// Handshake / commit packet (`cmd 0x0C`, 10 bytes).
pub fn build_commit_cmd() -> [u8; 10] {
    [0x0C, 0x0A, 0x01, 0xFE, 0x01, 0xFE, 0x00, 0x00, 0x00, 0x00]
}

/// Builds a 9-byte Polling Rate inner command (`cmd 0x06`, `INPHIC.exe 0x462260`).
pub fn build_polling_rate_cmd(hz: u16) -> Result<[u8; 9], String> {
    let code: u8 = match hz {
        125 => 0x20,
        250 => 0x10,
        500 => 0x08,
        1000 => 0x04,
        2000 => 0x02,
        4000 => 0x01,
        8000 => 0x40,
        _ => return Err(format!("不支持的 Inphic 回报率档位: {hz} Hz")),
    };
    Ok([0x06, 0x09, 0x01, code, !code, 0, 0, 0, 0])
}

/// Builds the 56-byte (`0x38`) 8-stage DPI + LOD + MotionSync command (`cmd 0x04`, `INPHIC.exe 0x461de0..0x462116`).
pub fn build_dpi_cmd(
    stages: &[(u16, [u8; 3])],
    active_stage_idx: usize,
    lod_mm: f32,
    motion_sync: bool,
) -> Result<[u8; 56], String> {
    if stages.is_empty() || stages.len() > 8 {
        return Err(format!(
            "DPI 档位数量必须在 1..=8 之间（实际 {}）",
            stages.len()
        ));
    }
    if active_stage_idx >= stages.len() {
        return Err(format!(
            "当前激活 DPI 档位索引越界（{active_stage_idx} >= {}）",
            stages.len()
        ));
    }

    let mut buf = [0u8; 56];
    let ms = u8::from(motion_sync);
    buf[0] = 0x04;
    buf[1] = 0x38;
    buf[2] = 0x01;
    buf[3] = ms;
    buf[4] = if lod_mm <= 1.0 { 1 } else { 2 };
    buf[5] = ((1u16 << stages.len()) - 1) as u8;
    buf[6] = ms;
    buf[7] = 0x01;

    for i in 0..8 {
        let (raw_dpi, rgb) = stages
            .get(i)
            .copied()
            .unwrap_or((800, [0x2B, 0x6B, 0x68]));
        let step = (raw_dpi.clamp(50, 26000) / 50).clamp(1, 520) - 1;
        buf[8 + i] = (step & 0xFF) as u8;
        buf[16 + i] = ((step >> 8) & 0xFF) as u8;
        let c_off = 25 + i * 3;
        buf[c_off..c_off + 3].copy_from_slice(&rgb);
    }

    buf[24] = (active_stage_idx + 1) as u8;
    buf[49] = 0x01;

    let inner_sum = buf[3..50]
        .iter()
        .fold(0u16, |acc, &b| acc.wrapping_add(b as u16));
    buf[50] = (inner_sum >> 8) as u8;
    buf[51] = (inner_sum & 0xFF) as u8;
    Ok(buf)
}

/// Builds an 8-byte key debounce inner command (`cmd 0x07`, `INPHIC.exe 0x461d61`).
pub fn build_debounce_cmd(debounce_ms: u8) -> [u8; 8] {
    let val = debounce_ms.min(16);
    [0x07, 0x08, val, !val, 0x00, 0xFF, 0x00, 0x00]
}

pub fn parse_hex_color(hex: &str) -> [u8; 3] {
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

fn parse_dpi_args_from_config(
    config: &serde_json::Value,
) -> (Vec<(u16, [u8; 3])>, usize, f32, bool, u16, u8) {
    let mut stages = Vec::new();
    if let Some(arr) = config.get("dpiStages").and_then(|v| v.as_array()) {
        for item in arr.iter().take(8) {
            let dpi = item
                .get("dpiX")
                .and_then(|v| v.as_u64())
                .unwrap_or(800)
                .clamp(50, 26000) as u16;
            let color = item
                .get("color")
                .and_then(|v| v.as_str())
                .map(parse_hex_color)
                .unwrap_or([0x2B, 0x6B, 0x68]);
            stages.push((dpi, color));
        }
    }
    if stages.is_empty() {
        stages.push((800, [0x2B, 0x6B, 0x68]));
    }

    let raw_idx = config
        .get("activeStageIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let active_idx = raw_idx.min(stages.len() - 1);
    let lod_mm = config
        .get("liftOffDistanceMm")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let motion_sync = config
        .get("motionSync")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let polling_hz = config
        .get("pollingRateHz")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as u16;
    let debounce_ms = config
        .get("debounceMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(4)
        .min(16) as u8;

    (stages, active_idx, lod_mm, motion_sync, polling_hz, debounce_ms)
}

/// Builds the 64-byte Report ID `0x04` DPI packet for UI preview or hardware dispatch.
pub fn build_dpi_frame_from_config(config: &serde_json::Value) -> Result<[u8; 64], String> {
    let (stages, active_idx, lod_mm, motion_sync, _, _) = parse_dpi_args_from_config(config);
    let dpi_cmd = build_dpi_cmd(&stages, active_idx, lod_mm, motion_sync)?;
    wrap_report04_frame(&dpi_cmd)
}

/// 5-layer pre-write anti-brick gate: never writes unless allowlisted, VID/PID + `col04` match,
/// AND passive COL03 telemetry confirmed live hardware sub-model `0x93` (Inphic IN10 / KP 8K).
pub fn validate_inphic_write_preconditions(
    vid: u16,
    pid: u16,
    control_path: &str,
    cached_model_id: Option<u8>,
) -> Result<(), String> {
    HardwareSafetyGuard::check_write_allowed(vid, pid, "inphic_in10_write")?;

    let lower = control_path.to_ascii_lowercase();
    if !lower.contains("vid_1d57&pid_fa65") || !lower.contains("col04") {
        return Err(format!(
            "安全护栏拦截：Inphic 控制接口必须是 vid_1d57&pid_fa65 COL04，拒绝写入 {control_path}"
        ));
    }

    match cached_model_id {
        Some(MODEL_ID_IN10) => Ok(()),
        Some(other) => Err(format!(
            "防盲写护栏拦截：检测到 Inphic 子型号 0x{other:02X}（当前写协议仅对 0x93 IN10/KP 8K 实机验证）"
        )),
        None => Err(
            "防盲写护栏等待：尚未从 COL03 遥测捕获鼠标型号 ID (0x93)，请移动或点击鼠标激活接收器后重试"
                .to_string(),
        ),
    }
}

fn read_expected_ack(dev: &HidDevice, expected: [u8; 2]) -> bool {
    let mut resp = [0u8; 64];
    for _ in 0..4 {
        match dev.read_timeout(&mut resp, 120) {
            Ok(len) if len >= 4 && resp[2] == expected[0] && resp[3] == expected[1] => {
                return true;
            }
            Ok(0) => continue,
            Ok(_) => continue,
            Err(_) => return false,
        }
    }
    false
}

fn write_frame_with_dual_ack(dev: &HidDevice, frame: &[u8; 64]) -> Result<(), String> {
    for _attempt in 0..3 {
        if dev.write(frame).is_err() {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        std::thread::sleep(Duration::from_millis(50));
        if !read_expected_ack(dev, [0xC0, 0x01]) {
            continue;
        }
        std::thread::sleep(Duration::from_millis(50));
        if read_expected_ack(dev, [0xC1, 0x01]) {
            return Ok(());
        }
    }
    Err(format!(
        "Inphic 指令 0x{:02X} 未收到硬件双阶段 ACK ([C0 01] / [C1 01])，已中止后续写入以保护鼠标",
        frame[3]
    ))
}

/// Applies validated DPI, LOD, MotionSync, Polling Rate, and Debounce settings to Inphic IN10 (`0x93`).
pub fn apply_inphic_in10_config(
    api: &HidApi,
    control_path: &str,
    config: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let cached = telemetry_daemon::get_cached_inphic_telemetry();
    validate_inphic_write_preconditions(
        VID_INPHIC,
        PID_KP8K_DONGLE,
        control_path,
        cached.model_id,
    )?;

    let (stages, active_idx, lod_mm, motion_sync, polling_hz, debounce_ms) =
        parse_dpi_args_from_config(config);

    let commit_frame = wrap_report04_frame(&build_commit_cmd())?;
    let dpi_frame = wrap_report04_frame(&build_dpi_cmd(&stages, active_idx, lod_mm, motion_sync)?)?;
    let polling_frame = wrap_report04_frame(&build_polling_rate_cmd(polling_hz)?)?;
    let debounce_frame = wrap_report04_frame(&build_debounce_cmd(debounce_ms))?;

    let c_path =
        CString::new(control_path).map_err(|_| "Inphic 控制路径包含非法空字符".to_string())?;
    let dev = api
        .open_path(c_path.as_c_str())
        .map_err(|e| format!("无法打开 Inphic COL04 控制通道: {e}"))?;

    for frame in [
        &commit_frame,
        &dpi_frame,
        &polling_frame,
        &debounce_frame,
        &commit_frame,
    ] {
        write_frame_with_dual_ack(&dev, frame)?;
    }

    Ok(dpi_frame.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_captured_in10_battery_packet() {
        let pkt = [0x03, 0x93, 0x40, 0x01, 0x3E];
        assert_eq!(
            parse_col03_report(&pkt),
            Some(InphicTelemetryEvent::Battery {
                model_id: 0x93,
                percent: Some(62),
                charging: false,
                online: true,
            })
        );
        assert_eq!(model_name_from_id(0x93), "Inphic IN10 (KP 8K)");
    }

    #[test]
    fn parses_charging_full_dpi_and_sleep_events() {
        assert_eq!(
            parse_col03_report(&[0x03, 0x93, 0x40, 0x02, 0x00]),
            Some(InphicTelemetryEvent::Battery {
                model_id: 0x93,
                percent: Some(100),
                charging: false,
                online: true,
            })
        );
        assert_eq!(
            parse_col03_report(&[0x03, 0x93, 0x40, 0x13, 0x4B]),
            Some(InphicTelemetryEvent::Battery {
                model_id: 0x93,
                percent: Some(75),
                charging: true,
                online: true,
            })
        );
        assert_eq!(
            parse_col03_report(&[0x03, 0x93, 0x10, 0x03, 0x00]),
            Some(InphicTelemetryEvent::DpiStageChanged {
                model_id: 0x93,
                stage_index: 2,
            })
        );
        assert_eq!(
            parse_col03_report(&[0x03, 0x93, 0x50, 0x00, 0x00]),
            Some(InphicTelemetryEvent::PowerState {
                model_id: 0x93,
                sleeping: true,
            })
        );
        assert_eq!(parse_col03_report(&[0x04, 0x93, 0x40, 0x01, 0x3E]), None);
    }

    #[test]
    fn report04_frame_wraps_checksum_and_blocks_macro_flash_opcode() {
        let commit = build_commit_cmd();
        let frame = wrap_report04_frame(&commit).unwrap();
        assert_eq!(frame[0], 0x04);
        assert_eq!(frame[1], 15); // 10 + 5
        assert_eq!(frame[2], 0x00);
        assert_eq!(&frame[3..13], &commit);
        let expected_sum: u16 = frame[0..13].iter().map(|&b| b as u16).sum();
        assert_eq!(frame[13], (expected_sum >> 8) as u8);
        assert_eq!(frame[14], (expected_sum & 0xFF) as u8);
        assert!(frame[15..].iter().all(|&b| b == 0));

        // Reject opcode 0x09 (macro flash) and unknown opcodes / oversize frames
        assert!(wrap_report04_frame(&[0x09, 0x08, 0x00]).is_err());
        assert!(wrap_report04_frame(&[0xFF, 0x05]).is_err());
        assert!(wrap_report04_frame(&[0x04; 59]).is_err());
    }

    #[test]
    fn encodes_all_polling_rates_and_debounce_with_complements() {
        let expected = [
            (125u16, 0x20u8),
            (250, 0x10),
            (500, 0x08),
            (1000, 0x04),
            (2000, 0x02),
            (4000, 0x01),
            (8000, 0x40),
        ];
        for (hz, code) in expected {
            let cmd = build_polling_rate_cmd(hz).unwrap();
            assert_eq!(cmd, [0x06, 0x09, 0x01, code, !code, 0, 0, 0, 0]);
        }
        assert!(build_polling_rate_cmd(333).is_err());

        let db4 = build_debounce_cmd(4);
        assert_eq!(db4, [0x07, 0x08, 4, !4u8, 0x00, 0xFF, 0x00, 0x00]);
        let db_clamped = build_debounce_cmd(99);
        assert_eq!(db_clamped, [0x07, 0x08, 16, !16u8, 0x00, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn encodes_dpi_stages_split_bytes_colors_and_inner_checksum() {
        let stages = vec![
            (50u16, [0x11, 0x22, 0x33]),
            (800, [0xAA, 0xBB, 0xCC]),
            (1600, [0x10, 0x20, 0x30]),
            (3200, [0x40, 0x50, 0x60]),
            (5950, [0x01, 0x02, 0x03]),
            (13000, [0x70, 0x80, 0x90]),
            (26000, [0xFE, 0xED, 0xFA]),
        ];
        let cmd = build_dpi_cmd(&stages, 2, 0.7, true).unwrap();
        assert_eq!(&cmd[0..8], &[0x04, 0x38, 0x01, 0x01, 0x01, 0x7F, 0x01, 0x01]);
        assert_eq!(cmd[24], 3); // 1-based active stage

        let expected_dpis = [50u16, 800, 1600, 3200, 5950, 13000, 26000];
        for (i, &dpi) in expected_dpis.iter().enumerate() {
            let step = (dpi / 50) - 1;
            assert_eq!(cmd[8 + i], (step & 0xFF) as u8);
            assert_eq!(cmd[16 + i], ((step >> 8) & 0xFF) as u8);
            assert_eq!(&cmd[25 + i * 3..28 + i * 3], &stages[i].1);
        }
        assert_eq!(cmd[49], 0x01);
        let inner_sum: u16 = cmd[3..50].iter().map(|&b| b as u16).sum();
        assert_eq!(cmd[50], (inner_sum >> 8) as u8);
        assert_eq!(cmd[51], (inner_sum & 0xFF) as u8);

        // LOD > 1.0 maps to code 2
        let cmd_high_lod = build_dpi_cmd(&[(800, [0, 0, 0])], 0, 2.0, false).unwrap();
        assert_eq!(cmd_high_lod[3], 0x00);
        assert_eq!(cmd_high_lod[4], 0x02);
        assert_eq!(cmd_high_lod[5], 0x01);
    }

    #[test]
    fn precondition_gate_enforces_model_0x93_col04_and_safety_guard() {
        let valid_path = r"\\?\hid#vid_1d57&pid_fa65&mi_02&col04#8&1234";
        let telemetry_path = r"\\?\hid#vid_1d57&pid_fa65&mi_02&col03#8&1234";

        assert!(validate_inphic_write_preconditions(
            VID_INPHIC,
            PID_KP8K_DONGLE,
            valid_path,
            Some(0x93),
        )
        .is_ok());

        // Missing or wrong model_id must block writes
        assert!(validate_inphic_write_preconditions(
            VID_INPHIC,
            PID_KP8K_DONGLE,
            valid_path,
            None,
        )
        .is_err());
        assert!(validate_inphic_write_preconditions(
            VID_INPHIC,
            PID_KP8K_DONGLE,
            valid_path,
            Some(0x95),
        )
        .is_err());

        // Wrong collection (COL03 instead of COL04) must block writes
        assert!(validate_inphic_write_preconditions(
            VID_INPHIC,
            PID_KP8K_DONGLE,
            telemetry_path,
            Some(0x93),
        )
        .is_err());

        // SinoWealth 0x258A is always blocked
        assert!(validate_inphic_write_preconditions(0x258A, 0x019D, valid_path, Some(0x93)).is_err());
    }
}
