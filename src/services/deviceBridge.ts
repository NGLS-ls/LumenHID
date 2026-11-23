import { EQ_FREQUENCIES } from '../data/seedDevices';
import type {
  AppProfileSnapshot,
  AudioConfig,
  BatterySource,
  KeyboardConfig,
  MacroDefinition,
  MouseConfig,
  PeripheralDevice,
  ProtocolSupportLevel,
} from '../types/peripheral';
import { classifyKnownPeripheral, isSystemEndpointName } from '../utils/deviceEngine';

const STORAGE_KEY = 'lumenhid.portable.profile.v2';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__?.invoke);
}

export function supportsWebHid(): boolean {
  return typeof navigator !== 'undefined' && 'hid' in navigator;
}

export async function invokeNativeCommand<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  try {
    return await window.__TAURI_INTERNALS__!.invoke<T>(cmd, args);
  } catch {
    return null;
  }
}

export function loadSavedProfile(): AppProfileSnapshot | null {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AppProfileSnapshot;
    if (parsed && parsed.schemaVersion === '2.0.0' && Array.isArray(parsed.devices)) {
      return parsed;
    }
    return null;
  } catch {
    return null;
  }
}

export async function saveProfileSnapshot(snapshot: AppProfileSnapshot): Promise<void> {
  const serialized = JSON.stringify(snapshot);
  try {
    window.localStorage.setItem(STORAGE_KEY, serialized);
  } catch {
    // Ignore storage quota issues in restricted iframes
  }
  if (isTauriRuntime()) {
    await invokeNativeCommand('save_portable_profile', {
      payload: serialized,
    });
  }
}

// Debounce timers prevent saturating HID I/O or disk writes while dragging sliders
let mouseSyncTimer: number | null = null;
let keyboardSyncTimer: number | null = null;
let audioSyncTimer: number | null = null;

export function syncMouseHardware(
  config: MouseConfig,
  device?: Pick<PeripheralDevice, 'vendorId' | 'productId' | 'vendorControlPath'> | null
): void {
  if (!isTauriRuntime() || !device) return;
  if (mouseSyncTimer !== null) window.clearTimeout(mouseSyncTimer);
  mouseSyncTimer = window.setTimeout(() => {
    void invokeNativeCommand('apply_mouse_config', {
      config: {
        ...config,
        vendorId: device.vendorId,
        productId: device.productId,
        vendorControlPath: device.vendorControlPath ?? null,
      },
    });
  }, 120);
}

export function syncKeyboardHardware(
  config: KeyboardConfig,
  device?: Pick<PeripheralDevice, 'vendorId' | 'productId'> | null
): void {
  if (!isTauriRuntime() || !device) return;
  if (keyboardSyncTimer !== null) window.clearTimeout(keyboardSyncTimer);
  keyboardSyncTimer = window.setTimeout(() => {
    void invokeNativeCommand('apply_keyboard_config', {
      config: { ...config, vendorId: device.vendorId, productId: device.productId },
    });
  }, 120);
}

export function syncAudioHardware(
  config: AudioConfig,
  device?: Pick<PeripheralDevice, 'vendorId' | 'productId'> | null
): void {
  if (!isTauriRuntime() || !device) return;
  if (audioSyncTimer !== null) window.clearTimeout(audioSyncTimer);
  audioSyncTimer = window.setTimeout(() => {
    void invokeNativeCommand('apply_audio_eq', {
      config: { ...config, vendorId: device.vendorId, productId: device.productId },
    });
  }, 120);
}

export async function triggerMacroHardware(macro: MacroDefinition): Promise<void> {
  if (isTauriRuntime()) {
    await invokeNativeCommand('execute_macro', { macroDef: macro });
  }
}

interface NativeDiscoveredPeripheral {
  vendor_id: string;
  product_id: string;
  product_name: string;
  manufacturer: string;
  interface_number: number;
  device_path: string;
  vendor_control_path: string | null;
  telemetry_path: string | null;
  led_path: string | null;
  protocol: '2.4g' | 'bluetooth' | 'usb';
  category: 'mouse' | 'keyboard' | 'headset';
  battery_percent: number | null;
  charging: boolean;
  battery_source: BatterySource;
  firmware_version: string | null;
  protocol_support: ProtocolSupportLevel;
  protocol_note: string;
  max_polling_hz: number;
}

export interface NativeBatterySnapshot {
  vendor_id: string;
  product_id: string;
  product_name: string;
  battery_percent: number | null;
  charging: boolean;
  battery_source: BatterySource;
  firmware_version: string | null;
}

export interface TelemetryEventPayload {
  vendor_id: string;
  product_id: string;
  product_name: string;
  battery_percent: number | null;
  charging: boolean;
  battery_source: BatterySource;
  firmware_version: string | null;
  active_dpi_stage: number | null;
  model_id: number | null;
}

export interface SafetyStatus {
  readonly_mode: boolean;
  blocked_vendors: string[];
  backup_count: number;
}

export interface BackupSnapshot {
  id: string;
  device_key: string;
  category: string;
  created_at_ms: number;
  source: string;
  payload: unknown;
}

const IMAGE_BY_CATEGORY = {
  mouse: '/src/assets/images/device_mouse_minimal_1795334434447.jpg',
  keyboard: '/src/assets/images/device_keyboard_minimal_1795334434542.jpg',
  headset: '/src/assets/images/device_headset_minimal_1795334435205.jpg',
} as const;

/**
 * Enumerates real physical Windows peripherals (HID + Bluetooth PnP battery)
 * with system endpoints (Realtek audio, touchpads, ACPI, hubs) stripped out.
 */
export async function scanNativeWindowsDevices(): Promise<PeripheralDevice[]> {
  if (!isTauriRuntime()) return [];
  const rawList = await invokeNativeCommand<NativeDiscoveredPeripheral[]>('scan_devices');
  if (!rawList || rawList.length === 0) return [];

  return rawList
    .filter((d) => !isSystemEndpointName(d.product_name, d.manufacturer))
    .slice(0, 16)
    .map((d) => {
      const category =
        d.category === 'keyboard' || d.category === 'headset' ? d.category : 'mouse';
      const protocol =
        d.protocol === '2.4g' || d.protocol === 'bluetooth' ? d.protocol : 'usb';
      const stableId = `hw-${d.vendor_id}-${d.product_id}-${d.product_name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;

      return {
        id: stableId,
        name: d.product_name,
        model: `${d.manufacturer} · Windows 原生端口`,
        category,
        activeProtocol: protocol,
        supportedProtocols: [protocol],
        connected: true,
        batteryPercent: d.battery_percent,
        batterySource: d.battery_source,
        charging: d.charging || d.battery_source === 'wired',
        firmwareVersion: d.firmware_version || (d.protocol === 'bluetooth' ? 'BT-PNP' : 'WIN-HID'),
        vendorId: d.vendor_id,
        productId: d.product_id,
        vendorControlPath: d.vendor_control_path,
        telemetryPath: d.telemetry_path,
        ledPath: d.led_path,
        protocolSupport: d.protocol_support,
        protocolNote: d.protocol_note,
        pollingRateHz: d.max_polling_hz || 1000,
        signalStrengthDbm: null,
        imageUrl: IMAGE_BY_CATEGORY[category],
        onboardSlots: 1,
        activeSlot: 1,
        sleepMinutes: 10,
      };
    });
}

export async function refreshNativeBatteries(): Promise<NativeBatterySnapshot[]> {
  if (!isTauriRuntime()) return [];
  const res = await invokeNativeCommand<NativeBatterySnapshot[]>('refresh_device_battery');
  return res ?? [];
}

export async function getSafetyStatus(): Promise<SafetyStatus | null> {
  return invokeNativeCommand<SafetyStatus>('get_safety_status');
}

export async function setReadonlyMode(enabled: boolean): Promise<SafetyStatus | null> {
  return invokeNativeCommand<SafetyStatus>('set_readonly_mode', { enabled });
}

export async function listDeviceBackups(deviceKey?: string): Promise<BackupSnapshot[]> {
  const res = await invokeNativeCommand<BackupSnapshot[]>('list_device_backups', {
    deviceKey: deviceKey ?? null,
  });
  return res ?? [];
}

/**
 * Scans real physical peripherals:
 * - On Windows EXE (Tauri): scans native HID + Bluetooth PnP asynchronously.
 * - On Web browser: invokes WebHID `requestDevice` and filters out system endpoints.
 */
export async function scanRealDevices(): Promise<{
  devices: PeripheralDevice[];
  message: string;
}> {
  if (isTauriRuntime()) {
    const found = await scanNativeWindowsDevices();
    if (found.length === 0) {
      return {
        devices: [],
        message: '未扫描到外部键鼠或蓝牙耳机（已自动屏蔽 Realtek 声卡与系统内置节点）。请确认外设已通电连接。',
      };
    }
    return {
      devices: found,
      message: `已识别 ${found.length} 台本机物理外设（已自动剔除系统音频与虚拟节点）。`,
    };
  }

  if (!supportsWebHid()) {
    return {
      devices: [],
      message: '当前浏览器不支持 WebHID。请下载并运行 Windows 桌面便携版 LumenHID.exe 以读取本机 2.4G / 蓝牙 / USB 外设与真实电量。',
    };
  }

  try {
    const navHid = (navigator as unknown as {
      hid: {
        requestDevice: (opts: {
          filters: unknown[];
        }) => Promise<Array<{ productName?: string; vendorId: number; productId: number }>>;
      };
    }).hid;
    const picked = await navHid.requestDevice({ filters: [] });
    if (!picked || picked.length === 0) {
      return {
        devices: [],
        message: '已取消浏览器 HID 设备授权。',
      };
    }

    const mapped: PeripheralDevice[] = [];
    for (const d of picked) {
      const vid = `0x${d.vendorId.toString(16).toUpperCase().padStart(4, '0')}`;
      const pid = `0x${d.productId.toString(16).toUpperCase().padStart(4, '0')}`;
      const rawName = d.productName?.trim() || `HID 外设 ${pid}`;
      if (isSystemEndpointName(rawName)) {
        continue;
      }
      const lower = rawName.toLowerCase();
      const fallbackCat =
        lower.includes('head') || lower.includes('audio') || lower.includes('hecate') || lower.includes('g5')
          ? 'headset'
          : lower.includes('key') || lower.includes('kbd') || lower.includes('k98')
            ? 'keyboard'
            : 'mouse';
      const info = classifyKnownPeripheral(vid, pid, rawName, fallbackCat);
      mapped.push({
        id: `webhid-${vid}-${pid}`,
        name: info.displayName,
        model: `${info.manufacturer} · WebHID 直连`,
        category: info.category,
        activeProtocol: 'usb',
        supportedProtocols: ['usb'],
        connected: true,
        batteryPercent: null,
        batterySource: 'unavailable',
        charging: false,
        firmwareVersion: 'WebHID',
        vendorId: vid,
        productId: pid,
        vendorControlPath: null,
        protocolSupport: info.protocolSupport,
        protocolNote: info.protocolNote,
        pollingRateHz: info.maxPollingHz,
        signalStrengthDbm: null,
        imageUrl: IMAGE_BY_CATEGORY[info.category],
        onboardSlots: 1,
        activeSlot: 1,
        sleepMinutes: 10,
      });
    }

    if (mapped.length === 0) {
      return {
        devices: [],
        message: '所选节点属于系统内置设备，已被自动过滤。请选择外接键鼠或耳机设备。',
      };
    }

    return {
      devices: mapped,
      message: `已通过 WebHID 接入 ${mapped.length} 台物理外设。`,
    };
  } catch {
    return {
      devices: [],
      message: '浏览器当前沙箱环境限制了 WebHID 授权窗口；请在 Windows 便携版 LumenHID.exe 中扫描本机硬件。',
    };
  }
}

export function downloadTextFile(filename: string, content: string, mimeType = 'text/plain;charset=utf-8'): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}

/**
 * Real Web Audio API 10-band BiquadFilter audition synthesizer so users can hear
 * EQ curve and stereo/spatial adjustments live.
 */
export class AudioPreviewEngine {
  private ctx: AudioContext | null = null;
  private filters: BiquadFilterNode[] = [];
  private preampGain: GainNode | null = null;
  private masterGain: GainNode | null = null;
  private panner: StereoPannerNode | null = null;
  private sourceNode: AudioNode | null = null;
  private oscNodes: OscillatorNode[] = [];
  private running = false;

  public start(bands: number[], preampDb: number, balance: number): boolean {
    if (this.running) return true;
    try {
      const AudioCtx =
        window.AudioContext ||
        (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      if (!AudioCtx) return false;
      const ctx = new AudioCtx();
      this.ctx = ctx;

      const preamp = ctx.createGain();
      preamp.gain.value = Math.pow(10, preampDb / 20);
      this.preampGain = preamp;

      const filters = EQ_FREQUENCIES.map((freq, idx) => {
        const filter = ctx.createBiquadFilter();
        filter.type =
          idx === 0 ? 'lowshelf' : idx === EQ_FREQUENCIES.length - 1 ? 'highshelf' : 'peaking';
        filter.frequency.value = freq;
        filter.Q.value = 1.4;
        filter.gain.value = bands[idx] ?? 0;
        return filter;
      });
      this.filters = filters;

      const panner = ctx.createStereoPanner();
      panner.pan.value = Math.max(-1, Math.min(1, balance / 100));
      this.panner = panner;

      const master = ctx.createGain();
      master.gain.value = 0.08;
      this.masterGain = master;

      let prev: AudioNode = preamp;
      for (const f of filters) {
        prev.connect(f);
        prev = f;
      }
      prev.connect(panner);
      panner.connect(master);
      master.connect(ctx.destination);

      const bufferSize = ctx.sampleRate * 2;
      const noiseBuffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
      const output = noiseBuffer.getChannelData(0);
      let b0 = 0,
        b1 = 0,
        b2 = 0,
        b3 = 0,
        b4 = 0,
        b5 = 0,
        b6 = 0;
      for (let i = 0; i < bufferSize; i++) {
        const white = Math.random() * 2 - 1;
        b0 = 0.99886 * b0 + white * 0.0555179;
        b1 = 0.99332 * b1 + white * 0.0750759;
        b2 = 0.969 * b2 + white * 0.153852;
        b3 = 0.8665 * b3 + white * 0.3104856;
        b4 = 0.55 * b4 + white * 0.5329522;
        b5 = -0.7616 * b5 - white * 0.016898;
        output[i] = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362) * 0.035;
        b6 = white * 0.115926;
      }

      const noise = ctx.createBufferSource();
      noise.buffer = noiseBuffer;
      noise.loop = true;
      noise.connect(preamp);
      noise.start();
      this.sourceNode = noise;

      const freqs = [130.81, 196.0, 329.63];
      this.oscNodes = freqs.map((f) => {
        const osc = ctx.createOscillator();
        const g = ctx.createGain();
        osc.type = 'sine';
        osc.frequency.value = f;
        g.gain.value = 0.18;
        osc.connect(g);
        g.connect(preamp);
        osc.start();
        return osc;
      });

      this.running = true;
      return true;
    } catch {
      this.stop();
      return false;
    }
  }

  public update(bands: number[], preampDb: number, balance: number): void {
    if (!this.running || !this.ctx) return;
    const now = this.ctx.currentTime;
    if (this.preampGain) {
      this.preampGain.gain.setTargetAtTime(Math.pow(10, preampDb / 20), now, 0.03);
    }
    this.filters.forEach((filter, idx) => {
      filter.gain.setTargetAtTime(bands[idx] ?? 0, now, 0.03);
    });
    if (this.panner) {
      this.panner.pan.setTargetAtTime(Math.max(-1, Math.min(1, balance / 100)), now, 0.03);
    }
  }

  public stop(): void {
    try {
      if (this.sourceNode && 'stop' in this.sourceNode) {
        (this.sourceNode as AudioBufferSourceNode).stop();
      }
      this.oscNodes.forEach((osc) => {
        try {
          osc.stop();
        } catch {
          // ignore
        }
      });
      if (this.ctx) {
        void this.ctx.close();
      }
    } catch {
      // ignore cleanup errors
    } finally {
      this.ctx = null;
      this.filters = [];
      this.preampGain = null;
      this.masterGain = null;
      this.panner = null;
      this.sourceNode = null;
      this.oscNodes = [];
      this.running = false;
    }
  }
}
