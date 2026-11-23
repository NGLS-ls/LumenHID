export type ConnectionProtocol = '2.4g' | 'bluetooth' | 'usb';
export type ProtocolFilter = 'all' | ConnectionProtocol;
export type DeviceCategory = 'mouse' | 'keyboard' | 'headset';
export type NavSection = 'devices' | 'mouse' | 'keyboard' | 'macros' | 'audio';

export type BatterySource = 'windows-bt' | 'hid-vendor' | 'wired' | 'unavailable';
export type ProtocolSupportLevel =
  | 'native-full'
  | 'native-battery-experimental-tuning'
  | 'native-battery-readonly'
  | 'soft-driver-safe'
  | 'system-apo';

export interface PeripheralDevice {
  id: string;
  name: string;
  model: string;
  category: DeviceCategory;
  activeProtocol: ConnectionProtocol;
  supportedProtocols: ConnectionProtocol[];
  connected: boolean;
  batteryPercent: number | null; // null when wired or hardware does not report battery
  batterySource: BatterySource;
  charging: boolean;
  firmwareVersion: string;
  vendorId: string; // e.g. "0x1D57"
  productId: string; // e.g. "0xFA65"
  vendorControlPath?: string | null;
  telemetryPath?: string | null;
  ledPath?: string | null;
  protocolSupport: ProtocolSupportLevel;
  protocolNote: string;
  pollingRateHz: number;
  signalStrengthDbm: number | null;
  imageUrl: string;
  onboardSlots: number;
  activeSlot: number;
  sleepMinutes: number;
}

export type BindingType =
  | 'default'
  | 'key'
  | 'mouse'
  | 'combo'
  | 'media'
  | 'macro'
  | 'dpi-cycle'
  | 'disabled';

export interface KeyBinding {
  type: BindingType;
  value: string;
  label: string;
  macroId?: string;
}

export interface DpiStage {
  id: string;
  dpiX: number;
  dpiY: number;
  color: string;
}

export interface MouseConfig {
  deviceId: string;
  activeStageIndex: number;
  independentXY: boolean;
  dpiStages: DpiStage[];
  pollingRateHz: 125 | 250 | 500 | 1000 | 2000 | 4000 | 8000;
  liftOffDistanceMm: 0.7 | 1.0 | 2.0;
  debounceMs: number;
  motionSync: boolean;
  angleSnapping: boolean;
  rippleControl: boolean;
  sleepMinutes: number;
  buttonBindings: Record<string, KeyBinding>;
}

export type KeyboardLayerId = 'base' | 'fn1' | 'game';
export type AmbientLightingMode = 'off' | 'static-mist' | 'breathing' | 'reactive';

export interface KeyCapDef {
  code: string;
  label: string;
  width?: number; // unit width in u (default 1)
  row: number;
}

export interface KeyboardConfig {
  deviceId: string;
  activeLayer: KeyboardLayerId;
  pollingRateHz: 1000 | 2000 | 4000 | 8000;
  globalActuationMm: number; // 0.1 to 4.0
  rapidTriggerEnabled: boolean;
  rapidTriggerSensitivityMm: number; // 0.05 to 2.0
  socdEnabled: boolean; // Snap Tap / Last Input Priority
  socdPairs: [string, string][];
  nkroEnabled: boolean;
  winKeyLocked: boolean;
  softDriverEnabled?: boolean;
  softwareDebounceMs?: number;
  lightingMode: AmbientLightingMode;
  lightingBrightness: number; // 0 to 100
  layerBindings: Record<KeyboardLayerId, Record<string, KeyBinding>>;
}

export type MacroEventKind = 'key-down' | 'key-up' | 'mouse-click' | 'delay';
export type MacroPlaybackMode = 'once' | 'while-held' | 'toggle' | 'repeat';

export interface MacroStep {
  id: string;
  kind: MacroEventKind;
  keyLabel: string;
  delayMs: number;
}

export interface MacroDefinition {
  id: string;
  name: string;
  description: string;
  playbackMode: MacroPlaybackMode;
  repeatCount: number;
  steps: MacroStep[];
  updatedAt: string;
}

export type RoomAcousticsMode = 'dry' | 'studio' | 'hall';

export interface SpatialChannel {
  id: 'FL' | 'C' | 'FR' | 'SL' | 'SR' | 'RL' | 'RR';
  label: string;
  angleDeg: number; // 0 = front center, clockwise
  gainDb: number; // -12 to +12
}

export interface AudioPreset {
  id: string;
  name: string;
  description: string;
  bands: number[]; // 10 bands: 31, 62, 125, 250, 500, 1k, 2k, 4k, 8k, 16k
  preampDb: number;
  builtIn: boolean;
}

export interface AudioConfig {
  deviceId: string;
  activePresetId: string;
  bands: number[]; // 10 values in dB (-12 to +12)
  preampDb: number; // -6 to +6
  surround71Enabled: boolean;
  soundstageWidth: number; // 0 to 100
  roomAcoustics: RoomAcousticsMode;
  bassBoost: number; // 0 to 100
  voiceClarity: number; // 0 to 100
  spatialChannels: SpatialChannel[];
  micGainPercent: number; // 0 to 100
  sidetonePercent: number; // 0 to 100
  noiseGateDb: number; // -60 to -10
  gameChatBalance: number; // -100 (Full Game) to +100 (Full Chat)
  customPresets: AudioPreset[];
}

export interface AppProfileSnapshot {
  schemaVersion: '2.0.0';
  exportedAt: string;
  activeDeviceId: string;
  devices: PeripheralDevice[];
  mouseConfig: MouseConfig;
  keyboardConfig: KeyboardConfig;
  macros: MacroDefinition[];
  audioConfig: AudioConfig;
}
