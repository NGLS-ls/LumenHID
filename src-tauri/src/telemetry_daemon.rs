//! Background telemetry listener daemon for passive HID devices (Inphic COL03 heartbeat/battery).

use crate::protocols::inphic_in10::{
    model_name_from_id, parse_col03_report, InphicTelemetryEvent, PID_KP8K_DONGLE, VID_INPHIC,
};
use crate::protocols::TelemetryEvent;
use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type TelemetryCallback = Arc<dyn Fn(TelemetryEvent) + Send + Sync>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InphicTelemetryCache {
    pub model_id: Option<u8>,
    pub model_name: Option<String>,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub active_dpi_stage: Option<u8>,
    pub sleeping: bool,
    pub last_updated_ms: u64,
}

static CACHE: OnceLock<Arc<RwLock<InphicTelemetryCache>>> = OnceLock::new();
static CALLBACK: OnceLock<RwLock<Option<TelemetryCallback>>> = OnceLock::new();
static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_PATH: OnceLock<RwLock<String>> = OnceLock::new();

fn cache_lock() -> &'static Arc<RwLock<InphicTelemetryCache>> {
    CACHE.get_or_init(|| Arc::new(RwLock::new(InphicTelemetryCache::default())))
}

fn callback_lock() -> &'static RwLock<Option<TelemetryCallback>> {
    CALLBACK.get_or_init(|| RwLock::new(None))
}

fn active_path_lock() -> &'static RwLock<String> {
    ACTIVE_PATH.get_or_init(|| RwLock::new(String::new()))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn get_cached_inphic_telemetry() -> InphicTelemetryCache {
    cache_lock()
        .read()
        .map(|g| g.clone())
        .unwrap_or_default()
}

pub fn update_cache_from_event(event: &InphicTelemetryEvent) -> InphicTelemetryCache {
    let mut guard = cache_lock()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match *event {
        InphicTelemetryEvent::Battery {
            model_id,
            percent,
            charging,
            ..
        } => {
            guard.model_id = Some(model_id);
            guard.model_name = Some(model_name_from_id(model_id).to_string());
            if let Some(p) = percent {
                guard.battery_percent = Some(p);
            }
            guard.charging = charging;
            guard.sleeping = false;
            guard.last_updated_ms = now_millis();
        }
        InphicTelemetryEvent::DpiStageChanged {
            model_id,
            stage_index,
        } => {
            guard.model_id = Some(model_id);
            guard.model_name = Some(model_name_from_id(model_id).to_string());
            guard.active_dpi_stage = Some(stage_index);
            guard.last_updated_ms = now_millis();
        }
        InphicTelemetryEvent::PowerState { model_id, sleeping } => {
            guard.model_id = Some(model_id);
            guard.model_name = Some(model_name_from_id(model_id).to_string());
            guard.sleeping = sleeping;
            guard.last_updated_ms = now_millis();
        }
    }
    guard.clone()
}

fn dispatch_telemetry_to_callback(snapshot: &InphicTelemetryCache) {
    let maybe_cb = callback_lock()
        .read()
        .ok()
        .and_then(|g| g.clone());
    let Some(cb) = maybe_cb else {
        return;
    };

    let payload = TelemetryEvent {
        vendor_id: format!("0x{:04X}", VID_INPHIC),
        product_id: format!("0x{:04X}", PID_KP8K_DONGLE),
        product_name: snapshot
            .model_name
            .clone()
            .unwrap_or_else(|| "Inphic IN10 / KP 8K".to_string()),
        battery_percent: snapshot.battery_percent,
        charging: snapshot.charging,
        battery_source: if snapshot.battery_percent.is_some() {
            "hid-vendor".to_string()
        } else {
            "unavailable".to_string()
        },
        firmware_version: None,
        active_dpi_stage: snapshot.active_dpi_stage,
        model_id: snapshot.model_id,
    };

    cb(payload);
}

/// Starts a single shared read-only background thread listening on Inphic COL03 (`UP=0x000A`).
pub fn ensure_inphic_listener(col03_path: String, on_event: Option<TelemetryCallback>) {
    if let Some(cb) = on_event {
        if let Ok(mut guard) = callback_lock().write() {
            *guard = Some(cb);
        }
    }

    if let Ok(mut path_guard) = active_path_lock().write() {
        *path_guard = col03_path;
    }

    if LISTENER_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }

    std::thread::Builder::new()
        .name("lumen-inphic-col03-telemetry".to_string())
        .spawn(move || {
            let mut backoff_ms: u64 = 1000;
            loop {
                let target_path = active_path_lock()
                    .read()
                    .map(|p| p.clone())
                    .unwrap_or_default();

                if target_path.is_empty() {
                    std::thread::sleep(Duration::from_millis(1000));
                    continue;
                }

                let opened = HidApi::new().ok().and_then(|api| {
                    CString::new(target_path.as_str())
                        .ok()
                        .and_then(|cp| api.open_path(cp.as_c_str()).ok().map(|dev| (api, dev)))
                });

                let Some((_api, dev)) = opened else {
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms * 2).min(5000);
                    continue;
                };

                backoff_ms = 1000;
                let mut buf = [0u8; 16];
                loop {
                    let current_path = active_path_lock()
                        .read()
                        .map(|p| p.clone())
                        .unwrap_or_default();
                    if current_path != target_path {
                        break;
                    }

                    match dev.read_timeout(&mut buf, 500) {
                        Ok(len) if len >= 5 => {
                            if let Some(evt) = parse_col03_report(&buf[..len]) {
                                let updated = update_cache_from_event(&evt);
                                dispatch_telemetry_to_callback(&updated);
                            }
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(500));
                            break;
                        }
                    }
                }
            }
        })
        .ok();
}
