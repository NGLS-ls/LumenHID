/// Standardized 16-byte HID Feature Report encoder for 2.4G / USB peripherals.
/// Layout: [0x08 ReportID, CommandID, PayloadLen, Payload[0..12], XOR Checksum]
pub const REPORT_ID_CONFIG: u8 = 0x08;

#[allow(dead_code)]
pub const CMD_SET_MOUSE_DPI: u8 = 0x21;
pub const CMD_SET_POLLING_RATE: u8 = 0x22;
pub const CMD_SET_KEYBOARD_ACTUATION: u8 = 0x31;
pub const CMD_SET_AUDIO_EQ: u8 = 0x41;

pub fn build_feature_report(command_id: u8, payload: &[u8]) -> [u8; 16] {
    let mut packet = [0u8; 16];
    packet[0] = REPORT_ID_CONFIG;
    packet[1] = command_id;
    let len = payload.len().min(12);
    packet[2] = len as u8;
    packet[3..3 + len].copy_from_slice(&payload[..len]);

    let mut checksum: u8 = 0;
    for byte in &packet[..15] {
        checksum ^= *byte;
    }
    packet[15] = checksum;
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_xor_of_first_15_bytes() {
        let pkt = build_feature_report(CMD_SET_MOUSE_DPI, &[0x06, 0x40, 0x0F, 0xA0]);
        assert_eq!(pkt[0], REPORT_ID_CONFIG);
        assert_eq!(pkt[1], CMD_SET_MOUSE_DPI);
        assert_eq!(pkt[2], 4);
        let expected: u8 = pkt[..15].iter().fold(0u8, |acc, b| acc ^ b);
        assert_eq!(pkt[15], expected);
    }
}
