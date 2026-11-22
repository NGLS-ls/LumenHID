import { EQ_FREQUENCIES } from '../data/seedDevices';
import type {
  AppProfileSnapshot,
  AudioConfig,
  KeyboardConfig,
  MacroDefinition,
  MouseConfig,
  PeripheralDevice,
} from '../types/peripheral';

const STORAGE_KEY = 'lumenhid.portable.profile.v1';

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
    if (parsed && parsed.schemaVersion === '1.0.0' && Array.isArray(parsed.devices)) {
      return parsed;
    }
    return null;
  } catch {
    return null;
  }
}

export async function saveProfileSnapshot(snapshot: AppProfileSnapshot): Promise<void> {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // Ignore storage quota issues in restricted iframes
  }
  if (isTauriRuntime()) {
    await invokeNativeCommand('save_portable_profile', {
      payload: JSON.stringify(snapshot),
    });
  }
}

export async function syncMouseHardware(config: MouseConfig): Promise<void> {
  if (isTauriRuntime()) {
    await invokeNativeCommand('apply_mouse_config', { config });
  }
}

export async function syncKeyboardHardware(config: KeyboardConfig): Promise<void> {
  if (isTauriRuntime()) {
    await invokeNativeCommand('apply_keyboard_config', { config });
  }
}

export async function syncAudioHardware(config: AudioConfig): Promise<void> {
  if (isTauriRuntime()) {
    await invokeNativeCommand('apply_audio_eq', { config });
  }
}

export async function triggerMacroHardware(macro: MacroDefinition): Promise<void> {
  if (isTauriRuntime()) {
    await invokeNativeCommand('execute_macro', { macro });
  }
}

/**
 * Attempts real WebHID enumeration when supported in a compatible desktop browser.
 */
export async function requestWebHidDevice(): Promise<PeripheralDevice | null> {
  if (!supportsWebHid()) return null;
  try {
    const navHid = (navigator as unknown as {
      hid: { requestDevice: (opts: { filters: unknown[] }) => Promise<Array<{ productName?: string; vendorId: number; productId: number }>> };
    }).hid;
    const picked = await navHid.requestDevice({ filters: [] });
    if (!picked || picked.length === 0) return null;
    const d = picked[0];
    const vid = `0x${d.vendorId.toString(16).toUpperCase().padStart(4, '0')}`;
    const pid = `0x${d.productId.toString(16).toUpperCase().padStart(4, '0')}`;
    const name = d.productName?.trim() || `HID 外设 ${pid}`;
    const lower = name.toLowerCase();
    const category =
      lower.includes('head') || lower.includes('audio') || lower.includes('mic')
        ? 'headset'
        : lower.includes('key') || lower.includes('kbd')
          ? 'keyboard'
          : 'mouse';
    const imageMap = {
      mouse: '/src/assets/images/device_mouse_minimal_1795334434447.jpg',
      keyboard: '/src/assets/images/device_keyboard_minimal_1795334434542.jpg',
      headset: '/src/assets/images/device_headset_minimal_1795334435205.jpg',
    };
    return {
      id: `webhid-${vid}-${pid}-${Date.now()}`,
      name,
      model: 'WebHID 标准直连外设',
      category,
      activeProtocol: 'usb',
      supportedProtocols: ['usb'],
      connected: true,
      batteryPercent: null,
      charging: false,
      firmwareVersion: 'HID-1.0',
      vendorId: vid,
      productId: pid,
      pollingRateHz: 1000,
      signalStrengthDbm: null,
      imageUrl: imageMap[category],
      onboardSlots: 1,
      activeSlot: 1,
      sleepMinutes: 10,
    };
  } catch {
    return null;
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
 * EQ curve and stereo/spatial adjustments live in-browser.
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
      const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      if (!AudioCtx) return false;
      const ctx = new AudioCtx();
      this.ctx = ctx;

      const preamp = ctx.createGain();
      preamp.gain.value = Math.pow(10, preampDb / 20);
      this.preampGain = preamp;

      const filters = EQ_FREQUENCIES.map((freq, idx) => {
        const filter = ctx.createBiquadFilter();
        filter.type = idx === 0 ? 'lowshelf' : idx === EQ_FREQUENCIES.length - 1 ? 'highshelf' : 'peaking';
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

      // Chain: source -> preamp -> filters[0..9] -> panner -> master -> destination
      let prev: AudioNode = preamp;
      for (const f of filters) {
        prev.connect(f);
        prev = f;
      }
      prev.connect(panner);
      panner.connect(master);
      master.connect(ctx.destination);

      // Gentle warm ambient harmonic chord + filtered soft pink noise for full-spectrum EQ audition
      const bufferSize = ctx.sampleRate * 2;
      const noiseBuffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
      const output = noiseBuffer.getChannelData(0);
      let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
      for (let i = 0; i < bufferSize; i++) {
        const white = Math.random() * 2 - 1;
        b0 = 0.99886 * b0 + white * 0.0555179;
        b1 = 0.99332 * b1 + white * 0.0750759;
        b2 = 0.96900 * b2 + white * 0.1538520;
        b3 = 0.86650 * b3 + white * 0.3104856;
        b4 = 0.55000 * b4 + white * 0.5329522;
        b5 = -0.7616 * b5 - white * 0.0168980;
        output[i] = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + white * 0.5362) * 0.035;
        b6 = white * 0.115926;
      }

      const noise = ctx.createBufferSource();
      noise.buffer = noiseBuffer;
      noise.loop = true;
      noise.connect(preamp);
      noise.start();
      this.sourceNode = noise;

      // Subtle pure harmonic triad (C3, G3, E4)
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
