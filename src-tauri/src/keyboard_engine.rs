//! Universal Windows user-mode keyboard soft-driver engine (`WH_KEYBOARD_LL` + `SendInput`).
//! Zero hardware Flash writes -> zero bricking risk across Bluetooth, 2.4G, and USB keyboards.
//! Includes hardcoded emergency panic (`Ctrl + Alt + Esc`), anti-stuck-key recovery, SOCD, and debounce.

use crate::safety::is_readonly_mode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Magic `dwExtraInfo` tag stamped onto every synthetic `SendInput` event so our hook ignores its own output.
pub const LUMEN_EXTRA_INFO: usize = 0x4C55_4D45;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardEngineStatus {
    pub engine_enabled: bool,
    pub hook_installed: bool,
    pub active_layer: String,
    pub active_bindings_count: usize,
    pub socd_enabled: bool,
    pub win_key_locked: bool,
    pub software_debounce_ms: u16,
    pub panic_triggered: bool,
    pub last_notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingAction {
    Key(u32),
    Combo(Vec<u32>),
    Media(u32),
    Macro(String),
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroStepAction {
    KeyDown(u32),
    KeyUp(u32),
    MouseClick(MouseButtonKind),
    Delay(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub struct MacroRuntime {
    pub id: String,
    pub playback_mode: String, // "once" | "while-held" | "toggle" | "repeat"
    pub repeat_count: u32,
    pub steps: Vec<(MacroStepAction, u64)>, // (action, post_delay_ms)
}

#[derive(Debug, Clone, Default)]
pub struct KeyboardRuntimeConfig {
    pub soft_driver_enabled: bool,
    pub active_layer: String,
    pub socd_enabled: bool,
    pub socd_pairs: Vec<(u32, u32)>,
    pub win_key_locked: bool,
    pub software_debounce_ms: u16,
    pub layer_bindings: HashMap<String, HashMap<u32, BindingAction>>,
}

impl KeyboardRuntimeConfig {
    pub fn active_layer_map(&self) -> Option<&HashMap<u32, BindingAction>> {
        self.layer_bindings.get(&self.active_layer)
    }

    pub fn active_bindings_count(&self) -> usize {
        self.active_layer_map().map(|m| m.len()).unwrap_or(0)
    }

    pub fn has_any_active_rule(&self) -> bool {
        self.active_bindings_count() > 0
            || self.socd_enabled
            || self.win_key_locked
            || self.software_debounce_ms > 0
    }
}

/// Maps JS `KeyboardEvent.code`, `KeyboardEvent.key`, or LumenHID key names to Windows Virtual-Key codes.
pub fn code_to_vk(code: &str) -> Option<u32> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Direct single ASCII letter or digit
    if trimmed.len() == 1 {
        let ch = trimmed.chars().next()?;
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_uppercase() as u32);
        }
        if ch.is_ascii_digit() {
            return Some(ch as u32);
        }
        return match ch {
            '`' | '~' => Some(0xC0),
            '-' | '_' => Some(0xBD),
            '=' | '+' => Some(0xBB),
            '[' | '{' => Some(0xDB),
            ']' | '}' => Some(0xDD),
            '\\' | '|' => Some(0xDC),
            ';' | ':' => Some(0xBA),
            '\'' | '"' => Some(0xDE),
            ',' | '<' => Some(0xBC),
            '.' | '>' => Some(0xBE),
            '/' | '?' => Some(0xBF),
            ' ' => Some(0x20),
            _ => None,
        };
    }

    if let Some(rest) = trimmed.strip_prefix("Key") {
        if rest.len() == 1 {
            let c = rest.chars().next()?;
            if c.is_ascii_alphabetic() {
                return Some(c.to_ascii_uppercase() as u32);
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("Digit") {
        if rest.len() == 1 {
            let c = rest.chars().next()?;
            if c.is_ascii_digit() {
                return Some(c as u32);
            }
        }
    }

    let upper = trimmed.to_ascii_uppercase();
    if let Some(fnum_str) = upper.strip_prefix('F') {
        if let Ok(n) = fnum_str.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + (n - 1));
            }
        }
    }

    match upper.as_str() {
        "ESCAPE" | "ESC" => Some(0x1B),
        "BACKSPACE" => Some(0x08),
        "TAB" => Some(0x09),
        "ENTER" | "RETURN" => Some(0x0D),
        "SPACE" | "SPACEBAR" => Some(0x20),
        "CAPSLOCK" | "CAPS" => Some(0x14),
        "DELETE" | "DEL" => Some(0x2E),
        "INSERT" | "INS" => Some(0x2D),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" | "PGUP" => Some(0x21),
        "PAGEDOWN" | "PGDN" => Some(0x22),
        "ARROWUP" | "UP" => Some(0x26),
        "ARROWDOWN" | "DOWN" => Some(0x28),
        "ARROWLEFT" | "LEFT" => Some(0x25),
        "ARROWRIGHT" | "RIGHT" => Some(0x27),
        "BACKQUOTE" => Some(0xC0),
        "MINUS" => Some(0xBD),
        "EQUAL" => Some(0xBB),
        "BRACKETLEFT" => Some(0xDB),
        "BRACKETRIGHT" => Some(0xDD),
        "BACKSLASH" => Some(0xDC),
        "SEMICOLON" => Some(0xBA),
        "QUOTE" => Some(0xDE),
        "COMMA" => Some(0xBC),
        "PERIOD" => Some(0xBE),
        "SLASH" => Some(0xBF),
        "CONTROLLEFT" | "CTRL" | "CONTROL" | "LCTRL" => Some(0xA2),
        "CONTROLRIGHT" | "RCTRL" => Some(0xA3),
        "SHIFTLEFT" | "SHIFT" | "LSHIFT" => Some(0xA0),
        "SHIFTRIGHT" | "RSHIFT" => Some(0xA1),
        "ALTLEFT" | "ALT" | "LALT" => Some(0xA4),
        "ALTRIGHT" | "RALT" => Some(0xA5),
        "METALEFT" | "META" | "WIN" | "LWIN" => Some(0x5B),
        "METARIGHT" | "RWIN" => Some(0x5C),
        "MEDIAPLAYPAUSE" => Some(0xB3),
        "MEDIANEXTTRACK" => Some(0xB0),
        "MEDIAPREVIOUSTRACK" => Some(0xB1),
        "MEDIASTOP" => Some(0xB2),
        "VOLUMEMUTE" => Some(0xAD),
        "VOLUMEDOWN" => Some(0xAE),
        "VOLUMEUP" => Some(0xAF),
        _ => None,
    }
}

pub fn parse_combo(expr: &str) -> Vec<u32> {
    expr.split('+')
        .filter_map(|part| code_to_vk(part.trim()))
        .collect()
}

pub fn is_win_vk(vk: u32) -> bool {
    vk == 0x5B || vk == 0x5C
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEventAction {
    Press(u32),
    Release(u32),
}

/// Last-Input-Priority SOCD (Simultaneous Opposite Cardinal Directions) state machine.
#[derive(Debug, Default)]
pub struct SocdState {
    pairs: Vec<(u32, u32)>,
    held: HashMap<u32, bool>,
    last_active: HashMap<usize, u32>,
}

impl SocdState {
    pub fn set_pairs(&mut self, pairs: Vec<(u32, u32)>) {
        if self.pairs != pairs {
            self.pairs = pairs;
            self.held.clear();
            self.last_active.clear();
        }
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.last_active.clear();
    }

    /// Processes a physical key event if `vk` belongs to a configured SOCD pair.
    /// Returns `None` if `vk` is not part of any SOCD pair.
    pub fn handle_key(&mut self, vk: u32, is_down: bool) -> Option<Vec<KeyEventAction>> {
        let mut matched_idx = None;
        let mut opposite_vk = 0u32;

        for (idx, &(a, b)) in self.pairs.iter().enumerate() {
            if vk == a {
                matched_idx = Some(idx);
                opposite_vk = b;
                break;
            }
            if vk == b {
                matched_idx = Some(idx);
                opposite_vk = a;
                break;
            }
        }

        let idx = matched_idx?;
        let was_held = self.held.get(&vk).copied().unwrap_or(false);
        let opp_held = self.held.get(&opposite_vk).copied().unwrap_or(false);
        let active = self.last_active.get(&idx).copied();

        if is_down {
            self.held.insert(vk, true);
            if !was_held {
                let mut actions = Vec::new();
                if opp_held && active == Some(opposite_vk) {
                    actions.push(KeyEventAction::Release(opposite_vk));
                }
                self.last_active.insert(idx, vk);
                actions.push(KeyEventAction::Press(vk));
                Some(actions)
            } else if active == Some(vk) {
                // Allow normal key auto-repeat for the currently active key
                Some(vec![KeyEventAction::Press(vk)])
            } else {
                // Suppress shadowed key auto-repeat while opposite key holds priority
                Some(Vec::new())
            }
        } else {
            self.held.insert(vk, false);
            if active == Some(vk) {
                let mut actions = vec![KeyEventAction::Release(vk)];
                if opp_held {
                    self.last_active.insert(idx, opposite_vk);
                    actions.push(KeyEventAction::Press(opposite_vk));
                } else {
                    self.last_active.remove(&idx);
                }
                Some(actions)
            } else {
                // Key was already released when the opposite key took priority
                Some(Vec::new())
            }
        }
    }
}

/// Software switch-chatter debounce filter.
#[derive(Debug, Default)]
pub struct DebounceTracker {
    last_release_ms: HashMap<u32, u64>,
    physically_held: HashSet<u32>,
}

impl DebounceTracker {
    pub fn clear(&mut self) {
        self.last_release_ms.clear();
        self.physically_held.clear();
    }

    pub fn should_suppress(&mut self, vk: u32, is_down: bool, now_ms: u64, debounce_ms: u16) -> bool {
        if debounce_ms == 0 {
            if is_down {
                self.physically_held.insert(vk);
            } else {
                self.physically_held.remove(&vk);
            }
            return false;
        }

        if is_down {
            if self.physically_held.contains(&vk) {
                // Key auto-repeat while already held down is never chatter
                return false;
            }
            if let Some(&last_up) = self.last_release_ms.get(&vk) {
                if now_ms.saturating_sub(last_up) < debounce_ms as u64 {
                    return true;
                }
            }
            self.physically_held.insert(vk);
            false
        } else {
            self.physically_held.remove(&vk);
            self.last_release_ms.insert(vk, now_ms);
            false
        }
    }
}

/// Tracks synthetic keys currently held down for active remaps so layer swaps or panic
/// never leave modifier or character keys stuck down.
#[derive(Debug, Default)]
pub struct StuckKeyTracker {
    pressed_remaps: HashMap<u32, Vec<u32>>,
}

impl StuckKeyTracker {
    pub fn record_press(&mut self, phys_vk: u32, emitted_vks: Vec<u32>) {
        if !emitted_vks.is_empty() {
            self.pressed_remaps.insert(phys_vk, emitted_vks);
        }
    }

    pub fn get_held(&self, phys_vk: u32) -> Option<&Vec<u32>> {
        self.pressed_remaps.get(&phys_vk)
    }

    pub fn take_release(&mut self, phys_vk: u32) -> Option<Vec<u32>> {
        self.pressed_remaps.remove(&phys_vk)
    }

    pub fn drain_all(&mut self) -> Vec<u32> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (_phys, vks) in self.pressed_remaps.drain() {
            for vk in vks.into_iter().rev() {
                if seen.insert(vk) {
                    out.push(vk);
                }
            }
        }
        out
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn parse_keyboard_config_json(val: &serde_json::Value) -> KeyboardRuntimeConfig {
    let soft_driver_enabled = val
        .get("softDriverEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let active_layer = val
        .get("activeLayer")
        .and_then(|v| v.as_str())
        .unwrap_or("base")
        .to_string();
    let socd_enabled = val
        .get("socdEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let win_key_locked = val
        .get("winKeyLocked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let software_debounce_ms = val
        .get("softwareDebounceMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(50) as u16;

    let mut socd_pairs = Vec::new();
    if let Some(pairs) = val.get("socdPairs").and_then(|v| v.as_array()) {
        for pair in pairs {
            if let Some(arr) = pair.as_array() {
                if arr.len() >= 2 {
                    if let (Some(a), Some(b)) = (
                        arr[0].as_str().and_then(code_to_vk),
                        arr[1].as_str().and_then(code_to_vk),
                    ) {
                        socd_pairs.push((a, b));
                    }
                }
            }
        }
    }
    if socd_pairs.is_empty() {
        socd_pairs.push((0x41, 0x44)); // Default A / D
    }

    let mut layer_bindings: HashMap<String, HashMap<u32, BindingAction>> = HashMap::new();
    if let Some(layers) = val.get("layerBindings").and_then(|v| v.as_object()) {
        for (layer_name, bindings_val) in layers {
            let mut map = HashMap::new();
            if let Some(bindings_obj) = bindings_val.as_object() {
                for (code_str, binding_obj) in bindings_obj {
                    let Some(src_vk) = code_to_vk(code_str) else {
                        continue;
                    };
                    let btype = binding_obj
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default");
                    let bval = binding_obj
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let action = match btype {
                        "disabled" => Some(BindingAction::Disabled),
                        "key" => code_to_vk(bval).map(BindingAction::Key),
                        "media" => code_to_vk(bval).map(BindingAction::Media),
                        "combo" => {
                            let vks = parse_combo(bval);
                            if vks.is_empty() {
                                None
                            } else {
                                Some(BindingAction::Combo(vks))
                            }
                        }
                        "macro" => {
                            let mid = binding_obj
                                .get("macroId")
                                .and_then(|v| v.as_str())
                                .unwrap_or(bval);
                            if mid.is_empty() {
                                None
                            } else {
                                Some(BindingAction::Macro(mid.to_string()))
                            }
                        }
                        _ => None,
                    };
                    if let Some(act) = action {
                        map.insert(src_vk, act);
                    }
                }
            }
            layer_bindings.insert(layer_name.clone(), map);
        }
    }

    KeyboardRuntimeConfig {
        soft_driver_enabled,
        active_layer,
        socd_enabled,
        socd_pairs,
        win_key_locked,
        software_debounce_ms,
        layer_bindings,
    }
}

pub fn parse_macro_definition_json(val: &serde_json::Value) -> Option<MacroRuntime> {
    let id = val.get("id")?.as_str()?.to_string();
    let playback_mode = val
        .get("playbackMode")
        .and_then(|v| v.as_str())
        .unwrap_or("once")
        .to_string();
    let repeat_count = val
        .get("repeatCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .clamp(1, 50) as u32;

    let mut steps = Vec::new();
    if let Some(arr) = val.get("steps").and_then(|v| v.as_array()) {
        for item in arr {
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let label = item.get("keyLabel").and_then(|v| v.as_str()).unwrap_or("");
            let delay_ms = item
                .get("delayMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(10_000);

            let action = match kind {
                "key-down" => code_to_vk(label).map(MacroStepAction::KeyDown),
                "key-up" => code_to_vk(label).map(MacroStepAction::KeyUp),
                "mouse-click" => {
                    let lower = label.to_ascii_lowercase();
                    let btn = if lower.contains("right") || label.contains("右") {
                        MouseButtonKind::Right
                    } else if lower.contains("middle") || label.contains("中") {
                        MouseButtonKind::Middle
                    } else {
                        MouseButtonKind::Left
                    };
                    Some(MacroStepAction::MouseClick(btn))
                }
                "delay" => Some(MacroStepAction::Delay(delay_ms)),
                _ => None,
            };

            if let Some(act) = action {
                steps.push((act, delay_ms));
            }
        }
    }

    Some(MacroRuntime {
        id,
        playback_mode,
        repeat_count,
        steps,
    })
}

// ============================================================================
// Global Engine State & Macro Execution Controller
// ============================================================================

pub type EngineNoticeCallback = Arc<dyn Fn(String) + Send + Sync>;

static ENGINE_ENABLED: AtomicBool = AtomicBool::new(true);
static PANIC_TRIGGERED: AtomicBool = AtomicBool::new(false);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static MACRO_EPOCH: AtomicU64 = AtomicU64::new(1);

static RUNTIME_CONFIG: OnceLock<RwLock<KeyboardRuntimeConfig>> = OnceLock::new();
static MACROS_STORE: OnceLock<RwLock<HashMap<String, MacroRuntime>>> = OnceLock::new();
static SOCD_STATE: OnceLock<Mutex<SocdState>> = OnceLock::new();
static DEBOUNCE_STATE: OnceLock<Mutex<DebounceTracker>> = OnceLock::new();
static STUCK_STATE: OnceLock<Mutex<StuckKeyTracker>> = OnceLock::new();
static LAST_NOTICE: OnceLock<RwLock<Option<String>>> = OnceLock::new();
static NOTICE_CALLBACK: OnceLock<RwLock<Option<EngineNoticeCallback>>> = OnceLock::new();

enum MacroWorkerMsg {
    Trigger {
        macro_def: MacroRuntime,
        epoch: u64,
    },
    StopAll,
}

static MACRO_TX: OnceLock<Mutex<Sender<MacroWorkerMsg>>> = OnceLock::new();
static ACTIVE_MACRO_TRIGGER_VK: OnceLock<Mutex<Option<(u32, String, u64)>>> = OnceLock::new();

fn config_lock() -> &'static RwLock<KeyboardRuntimeConfig> {
    RUNTIME_CONFIG.get_or_init(|| RwLock::new(KeyboardRuntimeConfig::default()))
}

fn macros_lock() -> &'static RwLock<HashMap<String, MacroRuntime>> {
    MACROS_STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn socd_lock() -> &'static Mutex<SocdState> {
    SOCD_STATE.get_or_init(|| Mutex::new(SocdState::default()))
}

fn debounce_lock() -> &'static Mutex<DebounceTracker> {
    DEBOUNCE_STATE.get_or_init(|| Mutex::new(DebounceTracker::default()))
}

fn stuck_lock() -> &'static Mutex<StuckKeyTracker> {
    STUCK_STATE.get_or_init(|| Mutex::new(StuckKeyTracker::default()))
}

fn last_notice_lock() -> &'static RwLock<Option<String>> {
    LAST_NOTICE.get_or_init(|| RwLock::new(None))
}

fn notice_cb_lock() -> &'static RwLock<Option<EngineNoticeCallback>> {
    NOTICE_CALLBACK.get_or_init(|| RwLock::new(None))
}

fn active_trigger_lock() -> &'static Mutex<Option<(u32, String, u64)>> {
    ACTIVE_MACRO_TRIGGER_VK.get_or_init(|| Mutex::new(None))
}

fn record_notice(msg: impl Into<String>) {
    let text = msg.into();
    if let Ok(mut g) = last_notice_lock().write() {
        *g = Some(text.clone());
    }
    if let Some(cb) = notice_cb_lock().read().ok().and_then(|g| g.clone()) {
        cb(text);
    }
}

pub fn set_notice_callback(cb: EngineNoticeCallback) {
    if let Ok(mut g) = notice_cb_lock().write() {
        *g = Some(cb);
    }
}

fn should_hook_be_active(cfg: &KeyboardRuntimeConfig) -> bool {
    ENGINE_ENABLED.load(Ordering::Acquire)
        && cfg.soft_driver_enabled
        && !is_readonly_mode()
        && cfg.has_any_active_rule()
}

fn release_all_stuck_keys() {
    let vks = stuck_lock()
        .lock()
        .map(|mut g| g.drain_all())
        .unwrap_or_default();
    for vk in vks {
        send_vk_event(vk, false);
    }
    if let Ok(mut socd) = socd_lock().lock() {
        socd.clear();
    }
    if let Ok(mut deb) = debounce_lock().lock() {
        deb.clear();
    }
}

fn stop_all_macros() {
    MACRO_EPOCH.fetch_add(1, Ordering::AcqRel);
    if let Ok(mut g) = active_trigger_lock().lock() {
        *g = None;
    }
    if let Some(tx) = MACRO_TX.get().and_then(|m| m.lock().ok()) {
        let _ = tx.send(MacroWorkerMsg::StopAll);
    }
}

fn get_macro_sender() -> &'static Mutex<Sender<MacroWorkerMsg>> {
    MACRO_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<MacroWorkerMsg>();
        std::thread::Builder::new()
            .name("lumen-macro-runner".to_string())
            .spawn(move || {
                loop {
                    let Ok(msg) = rx.recv() else {
                        break;
                    };
                    match msg {
                        MacroWorkerMsg::StopAll => continue,
                        MacroWorkerMsg::Trigger { macro_def, epoch } => {
                            execute_macro_loop(&macro_def, epoch, &rx);
                        }
                    }
                }
            })
            .ok();
        Mutex::new(tx)
    })
}

fn sleep_interruptible(ms: u64, epoch: u64) -> bool {
    let mut remaining = ms;
    while remaining > 0 {
        if SHUTDOWN_REQUESTED.load(Ordering::Acquire)
            || !ENGINE_ENABLED.load(Ordering::Acquire)
            || is_readonly_mode()
            || MACRO_EPOCH.load(Ordering::Acquire) != epoch
        {
            return false;
        }
        let slice = remaining.min(10);
        std::thread::sleep(Duration::from_millis(slice));
        remaining -= slice;
    }
    MACRO_EPOCH.load(Ordering::Acquire) == epoch
        && ENGINE_ENABLED.load(Ordering::Acquire)
        && !is_readonly_mode()
}

fn execute_macro_loop(macro_def: &MacroRuntime, epoch: u64, rx: &mpsc::Receiver<MacroWorkerMsg>) {
    let iterations: u32 = match macro_def.playback_mode.as_str() {
        "once" => 1,
        "repeat" => macro_def.repeat_count.max(1),
        "while-held" | "toggle" => u32::MAX,
        _ => 1,
    };

    let mut held_macro_vks: HashSet<u32> = HashSet::new();

    for _iter in 0..iterations {
        if MACRO_EPOCH.load(Ordering::Acquire) != epoch {
            break;
        }
        // Drain any non-blocking StopAll
        while let Ok(pending) = rx.try_recv() {
            if matches!(pending, MacroWorkerMsg::StopAll) {
                MACRO_EPOCH.fetch_add(1, Ordering::AcqRel);
                break;
            }
        }
        if MACRO_EPOCH.load(Ordering::Acquire) != epoch {
            break;
        }

        for (step, post_delay) in &macro_def.steps {
            if MACRO_EPOCH.load(Ordering::Acquire) != epoch
                || !ENGINE_ENABLED.load(Ordering::Acquire)
                || is_readonly_mode()
            {
                break;
            }

            match *step {
                MacroStepAction::KeyDown(vk) => {
                    held_macro_vks.insert(vk);
                    send_vk_event(vk, true);
                }
                MacroStepAction::KeyUp(vk) => {
                    held_macro_vks.remove(&vk);
                    send_vk_event(vk, false);
                }
                MacroStepAction::MouseClick(btn) => {
                    send_mouse_click(btn);
                }
                MacroStepAction::Delay(ms) => {
                    if !sleep_interruptible(ms, epoch) {
                        break;
                    }
                    continue;
                }
            }

            if *post_delay > 0 && !sleep_interruptible(*post_delay, epoch) {
                break;
            }
        }

        if iterations == u32::MAX && !sleep_interruptible(15, epoch) {
            break;
        }
    }

    // Release any macro keys still pressed if aborted mid-macro
    for vk in held_macro_vks {
        send_vk_event(vk, false);
    }

    if let Ok(mut g) = active_trigger_lock().lock() {
        if g.as_ref().map(|(_, _, ep)| *ep) == Some(epoch) {
            *g = None;
        }
    }
}

pub fn update_config(val: &serde_json::Value) -> KeyboardEngineStatus {
    let parsed = parse_keyboard_config_json(val);
    let old_layer = config_lock()
        .read()
        .map(|g| g.active_layer.clone())
        .unwrap_or_default();

    if old_layer != parsed.active_layer {
        release_all_stuck_keys();
    }

    if let Ok(mut socd) = socd_lock().lock() {
        socd.set_pairs(parsed.socd_pairs.clone());
    }
    if let Ok(mut g) = config_lock().write() {
        *g = parsed;
    }

    refresh_hook_state();
    get_status()
}

pub fn sync_macros(val: &serde_json::Value) -> usize {
    let mut map = HashMap::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            if let Some(m) = parse_macro_definition_json(item) {
                map.insert(m.id.clone(), m);
            }
        }
    }
    let count = map.len();
    if let Ok(mut g) = macros_lock().write() {
        *g = map;
    }
    count
}

pub fn run_macro_once(val: &serde_json::Value) -> Result<usize, String> {
    if is_readonly_mode() {
        return Err("当前处于全局只读安全模式，已跳过系统模拟宏注入".to_string());
    }
    let parsed = parse_macro_definition_json(val)
        .ok_or_else(|| "宏定义格式无效".to_string())?;
    let step_count = parsed.steps.len();
    if step_count == 0 {
        return Ok(0);
    }
    let single = MacroRuntime {
        playback_mode: "once".to_string(),
        ..parsed
    };
    let epoch = MACRO_EPOCH.fetch_add(1, Ordering::AcqRel) + 1;
    let sender = get_macro_sender();
    if let Ok(tx) = sender.lock() {
        let _ = tx.send(MacroWorkerMsg::Trigger {
            macro_def: single,
            epoch,
        });
    }
    Ok(step_count)
}

pub fn set_engine_enabled(enabled: bool) -> KeyboardEngineStatus {
    ENGINE_ENABLED.store(enabled, Ordering::Release);
    if enabled {
        PANIC_TRIGGERED.store(false, Ordering::Release);
        record_notice("键盘软驱动引擎已恢复运行");
    } else {
        stop_all_macros();
        release_all_stuck_keys();
        record_notice("键盘软驱动已切换至原厂直通暂停模式");
    }
    refresh_hook_state();
    get_status()
}

pub fn emergency_reset(reason: &str) -> KeyboardEngineStatus {
    ENGINE_ENABLED.store(false, Ordering::Release);
    PANIC_TRIGGERED.store(true, Ordering::Release);
    stop_all_macros();
    release_all_stuck_keys();
    record_notice(reason.to_string());
    refresh_hook_state();
    get_status()
}

pub fn get_status() -> KeyboardEngineStatus {
    let cfg = config_lock()
        .read()
        .map(|g| g.clone())
        .unwrap_or_default();
    KeyboardEngineStatus {
        engine_enabled: ENGINE_ENABLED.load(Ordering::Acquire),
        hook_installed: HOOK_INSTALLED.load(Ordering::Acquire),
        active_layer: cfg.active_layer.clone(),
        active_bindings_count: cfg.active_bindings_count(),
        socd_enabled: cfg.socd_enabled,
        win_key_locked: cfg.win_key_locked,
        software_debounce_ms: cfg.software_debounce_ms,
        panic_triggered: PANIC_TRIGGERED.load(Ordering::Acquire),
        last_notice: last_notice_lock().read().ok().and_then(|g| g.clone()),
    }
}

pub fn shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::Release);
    ENGINE_ENABLED.store(false, Ordering::Release);
    stop_all_macros();
    release_all_stuck_keys();
    refresh_hook_state();
}

// ============================================================================
// Windows Low-Level Hook & SendInput Implementation
// ============================================================================

#[cfg(target_os = "windows")]
mod win_hook {
    use super::*;
    use std::mem;

    const WH_KEYBOARD_LL: i32 = 13;
    const HC_ACTION: i32 = 0;
    const WM_KEYDOWN: usize = 0x0100;
    const WM_KEYUP: usize = 0x0101;
    const WM_SYSKEYDOWN: usize = 0x0104;
    const WM_SYSKEYUP: usize = 0x0105;
    const WM_APP_SYNC_HOOK: u32 = 0x8001;

    const INPUT_MOUSE: u32 = 0;
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_SCANCODE: u32 = 0x0008;
    const MAPVK_VK_TO_VSC: u32 = 0;

    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
    const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;

    #[repr(C)]
    struct KbdLlHookStruct {
        vk_code: u32,
        scan_code: u32,
        flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        mouse_data: u32,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        w_vk: u16,
        w_scan: u16,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct HardwareInput {
        u_msg: u32,
        w_param_l: u16,
        w_param_h: u16,
    }

    #[repr(C)]
    union InputUnion {
        mi: MouseInput,
        ki: KeybdInput,
        hi: HardwareInput,
    }

    #[repr(C)]
    struct Input {
        r#type: u32,
        u: InputUnion,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: Point,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowsHookExW(
            id_hook: i32,
            lpfn: unsafe extern "system" fn(i32, usize, isize) -> isize,
            hmod: isize,
            dw_thread_id: u32,
        ) -> isize;
        fn UnhookWindowsHookEx(hhk: isize) -> i32;
        fn CallNextHookEx(hhk: isize, n_code: i32, w_param: usize, l_param: isize) -> isize;
        fn GetMessageW(lp_msg: *mut Msg, h_wnd: isize, min: u32, max: u32) -> i32;
        fn PostThreadMessageW(id_thread: u32, msg: u32, w_param: usize, l_param: isize) -> i32;
        fn SendInput(c_inputs: u32, p_inputs: *const Input, cb_size: i32) -> u32;
        fn MapVirtualKeyW(u_code: u32, u_map_type: u32) -> u32;
        fn GetAsyncKeyState(v_key: i32) -> i16;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    static HOOK_THREAD_ID: AtomicU64 = AtomicU64::new(0);
    static WORKER_STARTED: AtomicBool = AtomicBool::new(false);
    static mut ACTIVE_HOOK: isize = 0;

    fn is_extended_vk(vk: u32) -> bool {
        matches!(
            vk,
            0x21 | 0x22
                | 0x23
                | 0x24
                | 0x25
                | 0x26
                | 0x27
                | 0x28
                | 0x2D
                | 0x2E
                | 0x5B
                | 0x5C
                | 0xA3
                | 0xA5
        )
    }

    fn is_media_vk(vk: u32) -> bool {
        matches!(vk, 0xAD..=0xB3)
    }

    pub fn send_vk_event(vk: u32, is_down: bool) {
        let mut flags = if is_down { 0 } else { KEYEVENTF_KEYUP };
        if is_extended_vk(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }

        let input = if is_media_vk(vk) {
            Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: vk as u16,
                        w_scan: 0,
                        dw_flags: flags,
                        time: 0,
                        dw_extra_info: LUMEN_EXTRA_INFO,
                    },
                },
            }
        } else {
            let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) } as u16;
            if scan != 0 {
                flags |= KEYEVENTF_SCANCODE;
            }
            Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: if scan != 0 { 0 } else { vk as u16 },
                        w_scan: scan,
                        dw_flags: flags,
                        time: 0,
                        dw_extra_info: LUMEN_EXTRA_INFO,
                    },
                },
            }
        };

        unsafe {
            SendInput(1, &input, mem::size_of::<Input>() as i32);
        }
    }

    pub fn send_mouse_click(btn: MouseButtonKind) {
        let (down_flag, up_flag) = match btn {
            MouseButtonKind::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            MouseButtonKind::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            MouseButtonKind::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        };
        let inputs = [
            Input {
                r#type: INPUT_MOUSE,
                u: InputUnion {
                    mi: MouseInput {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: down_flag,
                        time: 0,
                        dw_extra_info: LUMEN_EXTRA_INFO,
                    },
                },
            },
            Input {
                r#type: INPUT_MOUSE,
                u: InputUnion {
                    mi: MouseInput {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: up_flag,
                        time: 0,
                        dw_extra_info: LUMEN_EXTRA_INFO,
                    },
                },
            },
        ];
        unsafe {
            SendInput(2, inputs.as_ptr(), mem::size_of::<Input>() as i32);
        }
    }

    fn sync_hook_in_worker_thread() {
        let want_active = {
            let cfg = config_lock().read().map(|g| g.clone()).unwrap_or_default();
            !SHUTDOWN_REQUESTED.load(Ordering::Acquire) && should_hook_be_active(&cfg)
        };

        unsafe {
            if want_active && ACTIVE_HOOK == 0 {
                let h = SetWindowsHookExW(WH_KEYBOARD_LL, low_level_keyboard_proc, 0, 0);
                if h != 0 {
                    ACTIVE_HOOK = h;
                    HOOK_INSTALLED.store(true, Ordering::Release);
                }
            } else if !want_active && ACTIVE_HOOK != 0 {
                UnhookWindowsHookEx(ACTIVE_HOOK);
                ACTIVE_HOOK = 0;
                HOOK_INSTALLED.store(false, Ordering::Release);
            }
        }
    }

    pub fn refresh_hook_state() {
        if !WORKER_STARTED.swap(true, Ordering::AcqRel) {
            std::thread::Builder::new()
                .name("lumen-kbd-hook-loop".to_string())
                .spawn(|| unsafe {
                    let tid = GetCurrentThreadId();
                    HOOK_THREAD_ID.store(tid as u64, Ordering::Release);
                    sync_hook_in_worker_thread();

                    let mut msg: Msg = mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                        if msg.message == WM_APP_SYNC_HOOK {
                            sync_hook_in_worker_thread();
                        }
                    }
                })
                .ok();
        } else {
            let tid = HOOK_THREAD_ID.load(Ordering::Acquire) as u32;
            if tid != 0 {
                unsafe {
                    PostThreadMessageW(tid, WM_APP_SYNC_HOOK, 0, 0);
                }
            }
        }
    }

    fn dispatch_macro_from_hook(phys_vk: u32, is_down: bool, macro_id: &str) -> bool {
        let macro_opt = macros_lock()
            .read()
            .ok()
            .and_then(|m| m.get(macro_id).cloned());
        let Some(macro_def) = macro_opt else {
            return false;
        };

        if !is_down {
            // Release stops `while-held` macro if this key triggered it
            if let Ok(mut active) = active_trigger_lock().lock() {
                if let Some((tvk, ref mid, _)) = *active {
                    if tvk == phys_vk && mid == macro_id && macro_def.playback_mode == "while-held"
                    {
                        *active = None;
                        drop(active);
                        stop_all_macros();
                    }
                }
            }
            return true;
        }

        // Key-down trigger
        if macro_def.playback_mode == "toggle" {
            let already_running = active_trigger_lock()
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .map(|(tvk, mid, _)| tvk == phys_vk && mid == macro_id)
                .unwrap_or(false);
            if already_running {
                stop_all_macros();
                return true;
            }
        }

        let epoch = MACRO_EPOCH.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut active) = active_trigger_lock().lock() {
            *active = Some((phys_vk, macro_id.to_string(), epoch));
        }
        if let Ok(tx) = get_macro_sender().lock() {
            let _ = tx.send(MacroWorkerMsg::Trigger {
                macro_def,
                epoch,
            });
        }
        true
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        n_code: i32,
        w_param: usize,
        l_param: isize,
    ) -> isize {
        if n_code != HC_ACTION || l_param == 0 {
            return CallNextHookEx(0, n_code, w_param, l_param);
        }

        let kbd = &*(l_param as *const KbdLlHookStruct);

        // 1. Always pass through our own synthetic events immediately (prevent feedback loop)
        if kbd.dw_extra_info == LUMEN_EXTRA_INFO {
            return CallNextHookEx(0, n_code, w_param, l_param);
        }

        let is_down = w_param == WM_KEYDOWN || w_param == WM_SYSKEYDOWN;
        let is_up = w_param == WM_KEYUP || w_param == WM_SYSKEYUP;
        if !is_down && !is_up {
            return CallNextHookEx(0, n_code, w_param, l_param);
        }

        let vk = kbd.vk_code;

        // 2. Hardcoded emergency panic hotkey: Ctrl + Alt + Esc -> immediate bypass & stuck-key flush
        if is_down && vk == 0x1B {
            let ctrl_down = (GetAsyncKeyState(0x11) as u16 & 0x8000) != 0;
            let alt_down = (GetAsyncKeyState(0x12) as u16 & 0x8000) != 0;
            if ctrl_down && alt_down {
                emergency_reset("已触发紧急熔断 (Ctrl+Alt+Esc)，键盘已恢复原厂直通");
                return CallNextHookEx(0, n_code, w_param, l_param);
            }
        }

        if !ENGINE_ENABLED.load(Ordering::Acquire) || is_readonly_mode() {
            return CallNextHookEx(0, n_code, w_param, l_param);
        }

        let Ok(cfg) = config_lock().read() else {
            return CallNextHookEx(0, n_code, w_param, l_param);
        };

        // 3. Win-key lock
        if cfg.win_key_locked && is_win_vk(vk) {
            return 1;
        }

        // 4. Software switch-chatter debounce
        if cfg.software_debounce_ms > 0 {
            if let Ok(mut deb) = debounce_lock().lock() {
                if deb.should_suppress(vk, is_down, now_millis(), cfg.software_debounce_ms) {
                    return 1;
                }
            }
        }

        // 5. Key-up for an active remap must always release the exact synthetic keys pressed earlier
        if is_up {
            if let Ok(mut stuck) = stuck_lock().lock() {
                if let Some(emitted) = stuck.take_release(vk) {
                    for evk in emitted.into_iter().rev() {
                        send_vk_event(evk, false);
                    }
                    return 1;
                }
            }
        }

        // 6. Active layer remaps & macros
        if let Some(layer_map) = cfg.active_layer_map() {
            if let Some(action) = layer_map.get(&vk) {
                match action {
                    BindingAction::Disabled => return 1,
                    BindingAction::Key(target) | BindingAction::Media(target) => {
                        if is_down {
                            if let Ok(mut stuck) = stuck_lock().lock() {
                                if stuck.get_held(vk).is_none() {
                                    stuck.record_press(vk, vec![*target]);
                                }
                            }
                            send_vk_event(*target, true);
                        }
                        return 1;
                    }
                    BindingAction::Combo(targets) => {
                        if is_down {
                            if let Ok(mut stuck) = stuck_lock().lock() {
                                if stuck.get_held(vk).is_none() {
                                    stuck.record_press(vk, targets.clone());
                                }
                            }
                            for &t in targets {
                                send_vk_event(t, true);
                            }
                        }
                        return 1;
                    }
                    BindingAction::Macro(mid) => {
                        if dispatch_macro_from_hook(vk, is_down, mid) {
                            return 1;
                        }
                    }
                }
            }
        }

        // 7. SOCD priority handler (only when key isn't remapped away)
        if cfg.socd_enabled {
            if let Ok(mut socd) = socd_lock().lock() {
                if let Some(actions) = socd.handle_key(vk, is_down) {
                    for act in actions {
                        match act {
                            KeyEventAction::Press(tvk) => send_vk_event(tvk, true),
                            KeyEventAction::Release(tvk) => send_vk_event(tvk, false),
                        }
                    }
                    return 1;
                }
            }
        }

        CallNextHookEx(0, n_code, w_param, l_param)
    }
}

#[cfg(not(target_os = "windows"))]
mod win_hook {
    use super::*;

    pub fn send_vk_event(_vk: u32, _is_down: bool) {}
    pub fn send_mouse_click(_btn: MouseButtonKind) {}
    pub fn refresh_hook_state() {
        let cfg = config_lock().read().map(|g| g.clone()).unwrap_or_default();
        HOOK_INSTALLED.store(should_hook_be_active(&cfg), Ordering::Release);
    }
}

use win_hook::{refresh_hook_state, send_mouse_click, send_vk_event};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keycodes_and_combos_accurately() {
        assert_eq!(code_to_vk("CapsLock"), Some(0x14));
        assert_eq!(code_to_vk("ControlLeft"), Some(0xA2));
        assert_eq!(code_to_vk("Escape"), Some(0x1B));
        assert_eq!(code_to_vk("F13"), Some(0x7C));
        assert_eq!(code_to_vk("MediaPlayPause"), Some(0xB3));
        assert_eq!(code_to_vk("VolumeMute"), Some(0xAD));
        assert_eq!(code_to_vk("KeyA"), Some(0x41));
        assert_eq!(code_to_vk("Digit5"), Some(0x35));
        assert_eq!(code_to_vk("Space"), Some(0x20));
        assert_eq!(parse_combo("Ctrl+C"), vec![0xA2, 0x43]);
    }

    #[test]
    fn socd_executes_last_input_priority_counter_strafe() {
        let mut socd = SocdState::default();
        socd.set_pairs(vec![(0x41, 0x44)]); // A, D

        // Hold A -> Press A
        assert_eq!(
            socd.handle_key(0x41, true),
            Some(vec![KeyEventAction::Press(0x41)])
        );
        // While holding A, press D -> Release A then Press D
        assert_eq!(
            socd.handle_key(0x44, true),
            Some(vec![
                KeyEventAction::Release(0x41),
                KeyEventAction::Press(0x44)
            ])
        );
        // Auto-repeat of shadowed A while D is active -> suppressed
        assert_eq!(socd.handle_key(0x41, true), Some(vec![]));
        // Release D while A is still held -> Release D then re-Press A
        assert_eq!(
            socd.handle_key(0x44, false),
            Some(vec![
                KeyEventAction::Release(0x44),
                KeyEventAction::Press(0x41)
            ])
        );
        // Release A -> Release A
        assert_eq!(
            socd.handle_key(0x41, false),
            Some(vec![KeyEventAction::Release(0x41)])
        );
        // Unrelated key W -> None
        assert_eq!(socd.handle_key(0x57, true), None);
    }

    #[test]
    fn debounce_suppresses_chatter_but_allows_held_repeat() {
        let mut deb = DebounceTracker::default();
        assert!(!deb.should_suppress(0x45, true, 1000, 12));
        // Auto-repeat while still physically held -> NOT suppressed
        assert!(!deb.should_suppress(0x45, true, 1005, 12));
        // Physical release at 1050ms
        assert!(!deb.should_suppress(0x45, false, 1050, 12));
        // Bounce re-press 6ms later -> suppressed!
        assert!(deb.should_suppress(0x45, true, 1056, 12));
        // Normal press 20ms later -> allowed!
        assert!(!deb.should_suppress(0x45, true, 1070, 12));
    }

    #[test]
    fn stuck_key_tracker_drains_all_held_synthetic_keys() {
        let mut tracker = StuckKeyTracker::default();
        tracker.record_press(0x14, vec![0xA2, 0x43]);
        tracker.record_press(0x09, vec![0x1B]);
        assert_eq!(tracker.take_release(0x09), Some(vec![0x1B]));
        assert_eq!(tracker.drain_all(), vec![0x43, 0xA2]);
        assert!(tracker.drain_all().is_empty());
    }

    #[test]
    fn parses_keyboard_config_and_emergency_panic_resets_state() {
        let cfg_json = serde_json::json!({
            "activeLayer": "base",
            "socdEnabled": true,
            "socdPairs": [["KeyA", "KeyD"]],
            "winKeyLocked": true,
            "softwareDebounceMs": 8,
            "layerBindings": {
                "base": {
                    "CapsLock": { "type": "key", "value": "ControlLeft", "label": "左 Ctrl" }
                }
            }
        });
        let parsed = parse_keyboard_config_json(&cfg_json);
        assert!(parsed.has_any_active_rule());
        assert_eq!(parsed.software_debounce_ms, 8);
        assert_eq!(
            parsed.layer_bindings["base"].get(&0x14),
            Some(&BindingAction::Key(0xA2))
        );

        let st = emergency_reset("panic test");
        assert!(!st.engine_enabled);
        assert!(st.panic_triggered);
        assert!(!st.hook_installed);

        let resumed = set_engine_enabled(true);
        assert!(resumed.engine_enabled);
        assert!(!resumed.panic_triggered);
    }
}
