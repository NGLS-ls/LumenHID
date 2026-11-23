//! Inphic IN10 / KP 8K (`0x1D57:0xFA65`) passive COL03 telemetry parser & model table.
//! Verified against INPHIC.exe (`0x414b70`, `0x44f3a0`, `0x460a00`) and live hardware packet `03 93 40 01 3e`.

pub const VID_INPHIC: u16 = 0x1D57;
pub const PID_KP8K_DONGLE: u16 = 0xFA65;
#[allow(dead_code)]
pub const MODEL_ID_IN10: u8 = 0x93;

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
}
