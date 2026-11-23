//! Anti-brick hardware safety guard and atomic persistent backup/rollback store.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static READONLY_MODE: AtomicBool = AtomicBool::new(false);
#[allow(dead_code)]
static BACKUP_SEQ: AtomicU64 = AtomicU64::new(1);

#[allow(dead_code)]
pub const MAX_BACKUPS_PER_DEVICE: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupSnapshot {
    pub id: String,
    pub device_key: String,
    pub category: String,
    pub created_at_ms: u64,
    pub source: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyStatus {
    pub readonly_mode: bool,
    pub blocked_vendors: Vec<String>,
    pub backup_count: usize,
}

pub fn is_readonly_mode() -> bool {
    READONLY_MODE.load(Ordering::Acquire)
}

pub fn set_readonly_mode(enabled: bool) -> bool {
    READONLY_MODE.store(enabled, Ordering::Release);
    enabled
}

pub struct HardwareSafetyGuard;

impl HardwareSafetyGuard {
    /// Verifies whether a hardware write operation is allowed under anti-brick rules.
    pub fn check_write_allowed(vid: u16, pid: u16, op_desc: &str) -> Result<(), String> {
        if is_readonly_mode() {
            return Err(format!(
                "全局只读安全模式已开启，已拦截硬件写操作（{op_desc}）"
            ));
        }

        // Permanent hard block on SinoWealth (0x258A) Flash writes to prevent bricking.
        if vid == 0x258A {
            return Err(format!(
                "安全护栏已阻止写入 SinoWealth 0x258A 设备 Flash，防止变砖（{op_desc}）"
            ));
        }

        let op_lower = op_desc.to_ascii_lowercase();
        if op_lower.contains("ota")
            || op_lower.contains("firmware_upgrade")
            || op_lower.contains("bootloader")
            || op_lower.contains("flash_erase")
        {
            return Err(format!(
                "安全护栏禁止执行高危固件刷写或擦除操作（{op_desc}）"
            ));
        }

        let allowed = matches!(
            (vid, pid),
            (0x1D57, 0xFA65) | (0x35BB, 0xA00B) | (0x35BB, 0xA211)
        );
        if !allowed {
            return Err(format!(
                "设备 0x{vid:04X}:0x{pid:04X} 未列入已验证写协议白名单，已降级为只读保护（{op_desc}）"
            ));
        }

        Ok(())
    }

    pub fn current_status(base_dir: &Path) -> SafetyStatus {
        SafetyStatus {
            readonly_mode: is_readonly_mode(),
            blocked_vendors: vec!["0x258A (SinoWealth 固件安全只读)".to_string()],
            backup_count: list_backups(base_dir, None).len(),
        }
    }
}

#[allow(dead_code)]
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sanitize_key(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

fn backups_root(base_dir: &Path) -> PathBuf {
    base_dir.join("backups")
}

/// Saves a backup snapshot atomically via `.tmp` + `rename` and retains the newest 20 snapshots
/// (always preserving any `initial_baseline` snapshot).
#[allow(dead_code)]
pub fn save_backup(
    base_dir: &Path,
    device_key: &str,
    category: &str,
    source: &str,
    payload: serde_json::Value,
) -> Result<BackupSnapshot, String> {
    let safe_key = sanitize_key(device_key);
    let dir = backups_root(base_dir).join(&safe_key);
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;

    let ts = now_millis();
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
    let id = format!("{safe_key}_{ts}_{seq}");
    let snapshot = BackupSnapshot {
        id: id.clone(),
        device_key: safe_key,
        category: category.to_string(),
        created_at_ms: ts,
        source: source.to_string(),
        payload,
    };

    let json = serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?;
    let file_name = format!("{ts:016}_{seq:06}.json");
    let final_path = dir.join(&file_name);
    let tmp_path = dir.join(format!("{file_name}.tmp"));

    fs::write(&tmp_path, &json).map_err(|e| format!("写入临时备份失败: {e}"))?;
    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&tmp_path, &final_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("原子替换备份失败: {e}")
    })?;

    prune_device_backups(&dir, MAX_BACKUPS_PER_DEVICE);
    Ok(snapshot)
}

#[allow(dead_code)]
fn prune_device_backups(dir: &Path, max_keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut records: Vec<(PathBuf, BackupSnapshot)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(snap) = serde_json::from_str::<BackupSnapshot>(&raw) {
                records.push((path, snap));
            }
        }
    }

    if records.len() <= max_keep {
        return;
    }

    // Sort newest first; keep `initial_baseline` snapshots exempt from eviction when possible.
    records.sort_by(|a, b| b.1.created_at_ms.cmp(&a.1.created_at_ms));
    let mut non_baseline_kept = 0usize;
    for (path, snap) in records {
        if snap.source == "initial_baseline" {
            continue;
        }
        non_baseline_kept += 1;
        if non_baseline_kept > max_keep {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn list_backups(base_dir: &Path, device_key: Option<&str>) -> Vec<BackupSnapshot> {
    let root = backups_root(base_dir);
    if !root.exists() {
        return Vec::new();
    }

    let dirs: Vec<PathBuf> = match device_key {
        Some(key) => vec![root.join(sanitize_key(key))],
        None => fs::read_dir(&root)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
    };

    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(snap) = serde_json::from_str::<BackupSnapshot>(&raw) {
                    out.push(snap);
                }
            }
        }
    }

    out.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    out
}

#[allow(dead_code)]
pub fn load_latest_backup(base_dir: &Path, device_key: &str) -> Option<BackupSnapshot> {
    list_backups(base_dir, Some(device_key)).into_iter().next()
}

#[allow(dead_code)]
pub fn load_backup_by_id(base_dir: &Path, id: &str) -> Option<BackupSnapshot> {
    list_backups(base_dir, None)
        .into_iter()
        .find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static READONLY_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_test_dir(tag: &str) -> PathBuf {
        let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lumenhid_safety_{tag}_{}_{}", std::process::id(), seq));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hard_blocks_sinowealth_and_ota_operations() {
        let _guard = READONLY_TEST_LOCK.lock().unwrap();
        set_readonly_mode(false);

        let err_sw_bt = HardwareSafetyGuard::check_write_allowed(0x258A, 0x0040, "write_keymap").unwrap_err();
        assert!(err_sw_bt.contains("0x258A"));

        let err_sw_usb = HardwareSafetyGuard::check_write_allowed(0x258A, 0x019D, "write_rgb").unwrap_err();
        assert!(err_sw_usb.contains("0x258A"));

        let err_ota = HardwareSafetyGuard::check_write_allowed(0x1D57, 0xFA65, "ota_update").unwrap_err();
        assert!(err_ota.contains("高危"));

        let err_unknown = HardwareSafetyGuard::check_write_allowed(0x1234, 0x5678, "write_dpi").unwrap_err();
        assert!(err_unknown.contains("白名单"));

        assert!(HardwareSafetyGuard::check_write_allowed(0x1D57, 0xFA65, "write_polling_rate").is_ok());
        assert!(HardwareSafetyGuard::check_write_allowed(0x35BB, 0xA00B, "write_led").is_ok());
    }

    #[test]
    fn readonly_mode_blocks_all_hardware_writes() {
        let _guard = READONLY_TEST_LOCK.lock().unwrap();
        set_readonly_mode(true);
        let res = HardwareSafetyGuard::check_write_allowed(0x1D57, 0xFA65, "write_polling_rate");
        set_readonly_mode(false);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("只读安全模式"));
    }

    #[test]
    fn saves_lists_loads_and_prunes_atomic_backups() {
        let dir = temp_test_dir("backups");
        let base = save_backup(
            &dir,
            "inphic_0x1d57_0xfa65",
            "mouse",
            "initial_baseline",
            serde_json::json!({ "dpi": 800 }),
        )
        .unwrap();

        for i in 0..24u32 {
            save_backup(
                &dir,
                "inphic_0x1d57_0xfa65",
                "mouse",
                "pre_write",
                serde_json::json!({ "dpi": 1000 + i }),
            )
            .unwrap();
        }

        let list = list_backups(&dir, Some("inphic_0x1d57_0xfa65"));
        // 20 recent pre_write + 1 preserved initial_baseline
        assert_eq!(list.len(), 21);
        assert!(list.iter().any(|s| s.id == base.id));

        let latest = load_latest_backup(&dir, "inphic_0x1d57_0xfa65").unwrap();
        assert_eq!(latest.payload["dpi"], 1023);

        let by_id = load_backup_by_id(&dir, &base.id).unwrap();
        assert_eq!(by_id.payload["dpi"], 800);

        let _ = fs::remove_dir_all(&dir);
    }
}
