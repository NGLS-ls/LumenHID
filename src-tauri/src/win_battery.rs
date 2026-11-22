//! Windows native PnP Bluetooth & peripheral battery reader using zero-dependency
//! `setupapi.dll` FFI (`DEVPKEY_Bluetooth_Battery`, GUID `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid 2).
//! Executes in <5ms without spawning PowerShell or external processes.

#[derive(Debug, Clone)]
pub struct WinBluetoothDeviceInfo {
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub battery_percent: Option<u8>,
    pub hardware_id: String,
}

/// Parses `VID_xxxx` and `PID_xxxx` (or `VID&0002xxxx_PID&xxxx` for Bluetooth Classic/BLE)
/// from Windows PnP Hardware ID strings.
pub fn extract_vid_pid_from_hwid(hwid: &str) -> (Option<u16>, Option<u16>) {
    let upper = hwid.to_ascii_uppercase();
    let vid = find_hex_after_prefix(&upper, "VID_")
        .or_else(|| find_hex_after_prefix(&upper, "VID&0002"))
        .or_else(|| find_hex_after_prefix(&upper, "VID&0001"))
        .or_else(|| find_hex_after_prefix(&upper, "VID&"));
    let pid = find_hex_after_prefix(&upper, "PID_")
        .or_else(|| find_hex_after_prefix(&upper, "PID&"));
    (vid, pid)
}

fn find_hex_after_prefix(haystack: &str, prefix: &str) -> Option<u16> {
    let idx = haystack.find(prefix)?;
    let rest = &haystack[idx + prefix.len()..];
    let hex: String = rest
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() >= 4 {
        // Take last 4 hex digits if 6-8 digit BT vendor prefix was matched
        let slice = &hex[hex.len() - 4..];
        u16::from_str_radix(slice, 16).ok()
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{extract_vid_pid_from_hwid, WinBluetoothDeviceInfo};
    use std::mem;
    use std::ptr;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DevPropKey {
        fmtid: Guid,
        pid: u32,
    }

    #[repr(C)]
    struct SpDevinfoData {
        cb_size: u32,
        class_guid: Guid,
        dev_inst: u32,
        reserved: usize,
    }

    const DIGCF_PRESENT: u32 = 0x0000_0002;
    const DIGCF_ALLCLASSES: u32 = 0x0000_0004;
    const INVALID_HANDLE_VALUE: isize = -1;
    const DEVPROP_TYPE_BYTE: u32 = 0x0000_0003;
    const DEVPROP_TYPE_UINT32: u32 = 0x0000_0007;

    /// DEVPKEY_Bluetooth_Battery = {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2
    const DEVPKEY_BLUETOOTH_BATTERY: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0x104E_A319,
            data2: 0x6EE2,
            data3: 0x4701,
            data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5],
        },
        pid: 2,
    };

    /// DEVPKEY_Device_FriendlyName = {A45C254E-DF1C-4EFD-8020-67D146A850E0}, 14
    const DEVPKEY_DEVICE_FRIENDLY_NAME: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xA45C_254E,
            data2: 0xDF1C,
            data3: 0x4EFD,
            data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
        },
        pid: 14,
    };

    /// DEVPKEY_Device_DeviceDesc = {A45C254E-DF1C-4EFD-8020-67D146A850E0}, 2
    const DEVPKEY_DEVICE_DESC: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xA45C_254E,
            data2: 0xDF1C,
            data3: 0x4EFD,
            data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
        },
        pid: 2,
    };

    /// DEVPKEY_Device_HardwareIds = {A45C254E-DF1C-4EFD-8020-67D146A850E0}, 3
    const DEVPKEY_DEVICE_HARDWARE_IDS: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xA45C_254E,
            data2: 0xDF1C,
            data3: 0x4EFD,
            data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
        },
        pid: 3,
    };

    /// DEVPKEY_NAME = {B725F130-47EF-101A-A5F1-02608C9EEBAC}, 10
    const DEVPKEY_NAME: DevPropKey = DevPropKey {
        fmtid: Guid {
            data1: 0xB725_F130,
            data2: 0x47EF,
            data3: 0x101A,
            data4: [0xA5, 0xF1, 0x02, 0x60, 0x8C, 0x9E, 0xEB, 0xAC],
        },
        pid: 10,
    };

    #[link(name = "setupapi")]
    extern "system" {
        fn SetupDiGetClassDevsW(
            class_guid: *const Guid,
            enumerator: *const u16,
            hwnd_parent: isize,
            flags: u32,
        ) -> isize;
        fn SetupDiEnumDeviceInfo(
            device_info_set: isize,
            member_index: u32,
            device_info_data: *mut SpDevinfoData,
        ) -> i32;
        fn SetupDiGetDevicePropertyW(
            device_info_set: isize,
            device_info_data: *const SpDevinfoData,
            property_key: *const DevPropKey,
            property_type: *mut u32,
            property_buffer: *mut u8,
            property_buffer_size: u32,
            required_size: *mut u32,
            flags: u32,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(device_info_set: isize) -> i32;
    }

    unsafe fn read_utf16_property(
        dev_info: isize,
        dev_data: &SpDevinfoData,
        key: &DevPropKey,
    ) -> Option<String> {
        let mut prop_type: u32 = 0;
        let mut required_size: u32 = 0;
        SetupDiGetDevicePropertyW(
            dev_info,
            dev_data,
            key,
            &mut prop_type,
            ptr::null_mut(),
            0,
            &mut required_size,
            0,
        );
        if required_size == 0 || required_size > 16384 {
            return None;
        }
        let mut buf = vec![0u8; required_size as usize];
        let ok = SetupDiGetDevicePropertyW(
            dev_info,
            dev_data,
            key,
            &mut prop_type,
            buf.as_mut_ptr(),
            required_size,
            &mut required_size,
            0,
        );
        if ok == 0 {
            return None;
        }
        let u16s: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        // Join multi-sz strings with space and trim trailing nulls
        let joined = String::from_utf16_lossy(&u16s)
            .replace('\0', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    unsafe fn read_battery_property(dev_info: isize, dev_data: &SpDevinfoData) -> Option<u8> {
        let mut prop_type: u32 = 0;
        let mut required_size: u32 = 0;
        let mut buf = [0u8; 8];
        let ok = SetupDiGetDevicePropertyW(
            dev_info,
            dev_data,
            &DEVPKEY_BLUETOOTH_BATTERY,
            &mut prop_type,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut required_size,
            0,
        );
        if ok == 0 || required_size == 0 {
            return None;
        }
        let raw = if prop_type == DEVPROP_TYPE_BYTE {
            buf[0] as u32
        } else if prop_type == DEVPROP_TYPE_UINT32 && required_size >= 4 {
            u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
        } else {
            buf[0] as u32
        };
        if raw <= 100 {
            Some(raw as u8)
        } else {
            None
        }
    }

    pub fn query_windows_bluetooth_devices() -> Vec<WinBluetoothDeviceInfo> {
        let mut out = Vec::new();
        unsafe {
            let dev_info = SetupDiGetClassDevsW(
                ptr::null(),
                ptr::null(),
                0,
                DIGCF_PRESENT | DIGCF_ALLCLASSES,
            );
            if dev_info == INVALID_HANDLE_VALUE || dev_info == 0 {
                return out;
            }

            let mut index: u32 = 0;
            loop {
                let mut dev_data = SpDevinfoData {
                    cb_size: mem::size_of::<SpDevinfoData>() as u32,
                    class_guid: Guid {
                        data1: 0,
                        data2: 0,
                        data3: 0,
                        data4: [0; 8],
                    },
                    dev_inst: 0,
                    reserved: 0,
                };
                if SetupDiEnumDeviceInfo(dev_info, index, &mut dev_data) == 0 {
                    break;
                }
                index += 1;

                let battery = read_battery_property(dev_info, &dev_data);
                let hwid = read_utf16_property(dev_info, &dev_data, &DEVPKEY_DEVICE_HARDWARE_IDS)
                    .unwrap_or_default();

                // Only inspect devices that either report Bluetooth battery or are Bluetooth/BTHLE/USB Audio peripherals
                let hwid_upper = hwid.to_ascii_uppercase();
                let is_bt = hwid_upper.contains("BTHENUM")
                    || hwid_upper.contains("BTHLE")
                    || hwid_upper.contains("BLUETOOTH");
                if battery.is_none() && !is_bt {
                    continue;
                }

                let name = read_utf16_property(dev_info, &dev_data, &DEVPKEY_DEVICE_FRIENDLY_NAME)
                    .or_else(|| read_utf16_property(dev_info, &dev_data, &DEVPKEY_NAME))
                    .or_else(|| read_utf16_property(dev_info, &dev_data, &DEVPKEY_DEVICE_DESC))
                    .unwrap_or_default();
                if name.trim().is_empty() {
                    continue;
                }

                let (vendor_id, product_id) = extract_vid_pid_from_hwid(&hwid);
                out.push(WinBluetoothDeviceInfo {
                    name: name.trim().to_string(),
                    vendor_id,
                    product_id,
                    battery_percent: battery,
                    hardware_id: hwid,
                });
            }

            SetupDiDestroyDeviceInfoList(dev_info);
        }
        out
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::WinBluetoothDeviceInfo;

    pub fn query_windows_bluetooth_devices() -> Vec<WinBluetoothDeviceInfo> {
        Vec::new()
    }
}

pub fn query_windows_bluetooth_devices() -> Vec<WinBluetoothDeviceInfo> {
    imp::query_windows_bluetooth_devices()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_standard_and_bluetooth_vid_pid() {
        assert_eq!(
            extract_vid_pid_from_hwid(r"BTHENUM\{00001812-0000-1000-8000-00805f9b34fb}_VID&0002258A_PID&0040"),
            (Some(0x258A), Some(0x0040))
        );
        assert_eq!(
            extract_vid_pid_from_hwid(r"USB\VID_1D57&PID_FA65&REV_0100"),
            (Some(0x1D57), Some(0xFA65))
        );
        assert_eq!(
            extract_vid_pid_from_hwid(r"BTHENUM\Dev_A4C138112233"),
            (None, None)
        );
    }
}
