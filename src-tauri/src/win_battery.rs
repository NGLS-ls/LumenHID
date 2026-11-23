//! Windows native PnP Bluetooth & peripheral battery reader using zero-dependency
//! `setupapi.dll` FFI (`DEVPKEY_Bluetooth_Battery`, GUID `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid 2).
//! Executes in <5ms without spawning PowerShell or external processes.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct WinBluetoothDeviceInfo {
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub battery_percent: Option<u8>,
    pub hardware_id: String,
    pub ble_mac: Option<String>,
}

/// Parses `VID_xxxx` and `PID_xxxx` (or `VID&0002xxxx_PID&xxxx` for Bluetooth Classic/BLE)
/// from Windows PnP Hardware ID or Instance ID strings.
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

/// Extracts a 12-hex BLE/Bluetooth device address (MAC) from Windows PnP InstanceId / HID path,
/// such as `BTHLE\DEV_FF0F00191294\...` or `..._REV&0300_FF0F00191294\...`.
pub fn extract_ble_mac(text: &str) -> Option<String> {
    let upper = text.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        let matched_prefix = if bytes[i..].starts_with(b"DEV_") {
            Some(i + 4)
        } else if bytes[i] == b'_' {
            Some(i + 1)
        } else {
            None
        };

        if let Some(start) = matched_prefix {
            if start + 12 <= len {
                let candidate = &bytes[start..start + 12];
                let all_hex = candidate.iter().all(|b| b.is_ascii_hexdigit());
                let valid_end = start + 12 == len
                    || matches!(
                        bytes[start + 12],
                        b'\\' | b'#' | b'&' | b' ' | b'\0'
                    );
                if all_hex && valid_end {
                    return std::str::from_utf8(candidate).ok().map(str::to_string);
                }
            }
        }
    }
    None
}

fn is_generic_bt_service_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("通用属性")
        || lower.contains("通用访问")
        || lower.contains("设备信息服务")
        || lower.contains("generic attribute")
        || lower.contains("generic access")
        || lower.contains("device information service")
        || lower.contains("符合蓝牙低能耗 gatt")
        || lower.contains("microsoft bluetooth")
}

/// Coalesces sibling PnP nodes sharing the same `ble_mac` (e.g. `BTHLE\DEV_<MAC>` holding
/// friendly name + battery, and `BTHLEDEVICE\{00001812-...}_..._<MAC>` holding VID/PID).
pub fn coalesce_bt_devices(devices: Vec<WinBluetoothDeviceInfo>) -> Vec<WinBluetoothDeviceInfo> {
    let mut by_mac: HashMap<String, usize> = HashMap::new();
    let mut merged: Vec<WinBluetoothDeviceInfo> = Vec::new();

    for dev in devices {
        if let Some(ref mac) = dev.ble_mac {
            if let Some(&idx) = by_mac.get(mac) {
                let existing = &mut merged[idx];
                if existing.vendor_id.is_none() {
                    existing.vendor_id = dev.vendor_id;
                }
                if existing.product_id.is_none() {
                    existing.product_id = dev.product_id;
                }
                if existing.battery_percent.is_none() && dev.battery_percent.is_some() {
                    existing.battery_percent = dev.battery_percent;
                }
                if (is_generic_bt_service_name(&existing.name) || existing.name.is_empty())
                    && !is_generic_bt_service_name(&dev.name)
                    && !dev.name.is_empty()
                {
                    existing.name = dev.name;
                    existing.hardware_id = dev.hardware_id;
                }
                continue;
            }
            by_mac.insert(mac.clone(), merged.len());
        }
        merged.push(dev);
    }

    merged
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
    use super::{
        coalesce_bt_devices, extract_ble_mac, extract_vid_pid_from_hwid, WinBluetoothDeviceInfo,
    };
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
        fn SetupDiGetDeviceInstanceIdW(
            device_info_set: isize,
            device_info_data: *const SpDevinfoData,
            device_instance_id: *mut u16,
            device_instance_id_size: u32,
            required_size: *mut u32,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(device_info_set: isize) -> i32;
    }

    unsafe fn read_instance_id(dev_info: isize, dev_data: &SpDevinfoData) -> Option<String> {
        let mut buf = [0u16; 512];
        let mut req_len: u32 = 0;
        let ok = SetupDiGetDeviceInstanceIdW(
            dev_info,
            dev_data,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut req_len,
        );
        if ok == 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
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
                let instance_id = read_instance_id(dev_info, &dev_data).unwrap_or_default();

                let id_upper = format!("{hwid} {instance_id}").to_ascii_uppercase();
                let is_bt = id_upper.contains("BTHENUM")
                    || id_upper.contains("BTHLE")
                    || id_upper.contains("BLUETOOTH");
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

                let (v1, p1) = extract_vid_pid_from_hwid(&hwid);
                let (v2, p2) = extract_vid_pid_from_hwid(&instance_id);
                let vendor_id = v1.or(v2);
                let product_id = p1.or(p2);
                let ble_mac = extract_ble_mac(&instance_id).or_else(|| extract_ble_mac(&hwid));

                out.push(WinBluetoothDeviceInfo {
                    name: name.trim().to_string(),
                    vendor_id,
                    product_id,
                    battery_percent: battery,
                    hardware_id: if instance_id.is_empty() {
                        hwid
                    } else {
                        instance_id
                    },
                    ble_mac,
                });
            }

            SetupDiDestroyDeviceInfoList(dev_info);
        }
        coalesce_bt_devices(out)
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
            extract_vid_pid_from_hwid(
                r"BTHENUM\{00001812-0000-1000-8000-00805f9b34fb}_VID&0002258A_PID&0040"
            ),
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

    #[test]
    fn extracts_ble_mac_from_pnp_and_hid_paths() {
        assert_eq!(
            extract_ble_mac(r"BTHLE\DEV_FF0F00191294\B&20345D26&0&FF0F00191294"),
            Some("FF0F00191294".to_string())
        );
        assert_eq!(
            extract_ble_mac(
                r"BTHLEDEVICE\{00001812-0000-1000-8000-00805F9B34FB}_DEV_VID&02258A_PID&0040_REV&0300_FF0F00191294\C&3616C6DE&0&001E"
            ),
            Some("FF0F00191294".to_string())
        );
        assert_eq!(
            extract_ble_mac(
                r"\\?\hid#{00001812-0000-1000-8000-00805f9b34fb}_dev_vid&02258a_pid&0040_rev&0300_ff0f00191294#c&3616c6de&0&001e"
            ),
            Some("FF0F00191294".to_string())
        );
        assert_eq!(
            extract_ble_mac(r"\\?\hid#vid_1d57&pid_fa65&mi_02&col03#a&d41e9db&0&0002"),
            None
        );
    }

    #[test]
    fn coalesces_sibling_bt_nodes_by_ble_mac() {
        let raw = vec![
            WinBluetoothDeviceInfo {
                name: "K98PRO 5.0".to_string(),
                vendor_id: None,
                product_id: None,
                battery_percent: Some(19),
                hardware_id: r"BTHLE\DEV_FF0F00191294\B&20345D26&0&FF0F00191294".to_string(),
                ble_mac: Some("FF0F00191294".to_string()),
            },
            WinBluetoothDeviceInfo {
                name: "符合蓝牙低能耗 GATT 的 HID 设备".to_string(),
                vendor_id: Some(0x258A),
                product_id: Some(0x0040),
                battery_percent: None,
                hardware_id: r"BTHLEDEVICE\{00001812-0000-1000-8000-00805F9B34FB}_DEV_VID&02258A_PID&0040_REV&0300_FF0F00191294".to_string(),
                ble_mac: Some("FF0F00191294".to_string()),
            },
        ];
        let merged = coalesce_bt_devices(raw);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "K98PRO 5.0");
        assert_eq!(merged[0].vendor_id, Some(0x258A));
        assert_eq!(merged[0].product_id, Some(0x0040));
        assert_eq!(merged[0].battery_percent, Some(19));
    }
}
