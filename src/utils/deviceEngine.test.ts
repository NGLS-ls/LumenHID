import test from 'node:test';
import assert from 'node:assert/strict';
import {
  addDpiStage,
  applyEqPreset,
  buildHidFeatureReport,
  calculateMacroDuration,
  calculateSpatialNodePosition,
  clampDpi,
  clampEqGain,
  classifyKnownPeripheral,
  filterDevices,
  generateEqCurvePath,
  generateEqualizerApoConfig,
  isSystemEndpointName,
  normalizeMacroDelays,
  parseAndValidateProfile,
  removeDpiStage,
  serializeProfile,
  updateDpiStage,
  validateActuation,
  validateMacroName,
} from './deviceEngine';
import {
  BUILTIN_EQ_PRESETS,
  DEFAULT_AUDIO_CONFIG,
  DEFAULT_KEYBOARD_CONFIG,
  DEFAULT_MOUSE_CONFIG,
} from '../data/seedDevices';
import type { MacroStep, PeripheralDevice } from '../types/peripheral';

const FIXTURE_DEVICES: PeripheralDevice[] = [
  {
    id: 'hw-0x1d57-0xfa65',
    name: 'Inphic KP 8K',
    model: '英菲克 Inphic · Windows 原生端口',
    category: 'mouse',
    activeProtocol: '2.4g',
    supportedProtocols: ['2.4g', 'usb'],
    connected: true,
    batteryPercent: 82,
    batterySource: 'hid-vendor',
    charging: false,
    firmwareVersion: 'WIN-HID',
    vendorId: '0x1D57',
    productId: '0xFA65',
    protocolSupport: 'native-battery-experimental-tuning',
    protocolNote: 'test',
    pollingRateHz: 8000,
    signalStrengthDbm: null,
    imageUrl: '',
    onboardSlots: 1,
    activeSlot: 1,
    sleepMinutes: 10,
  },
  {
    id: 'hw-0x258a-0x0040',
    name: '凌豹 K98PRO 5.0',
    model: '凌豹 / Sinowealth · Windows 原生端口',
    category: 'keyboard',
    activeProtocol: 'bluetooth',
    supportedProtocols: ['bluetooth'],
    connected: true,
    batteryPercent: 76,
    batterySource: 'windows-bt',
    charging: false,
    firmwareVersion: 'BT-PNP',
    vendorId: '0x258A',
    productId: '0x0040',
    protocolSupport: 'native-battery-readonly',
    protocolNote: 'test',
    pollingRateHz: 1000,
    signalStrengthDbm: null,
    imageUrl: '',
    onboardSlots: 1,
    activeSlot: 1,
    sleepMinutes: 10,
  },
  {
    id: 'hw-g5-max',
    name: '漫步者 HECATE G5 Max',
    model: '漫步者 Edifier HECATE · Windows 原生端口',
    category: 'headset',
    activeProtocol: 'bluetooth',
    supportedProtocols: ['bluetooth'],
    connected: true,
    batteryPercent: 91,
    batterySource: 'windows-bt',
    charging: false,
    firmwareVersion: 'BT-PNP',
    vendorId: 'BT-PNP',
    productId: 'BT-AUDIO',
    protocolSupport: 'system-apo',
    protocolNote: 'test',
    pollingRateHz: 1000,
    signalStrengthDbm: null,
    imageUrl: '',
    onboardSlots: 1,
    activeSlot: 1,
    sleepMinutes: 10,
  },
];

test('isSystemEndpointName strictly blocks Realtek & system endpoints while allowing real peripherals', () => {
  assert.equal(isSystemEndpointName('Realtek 高清音频输出 (扬声器/耳麦)', 'Realtek'), true);
  assert.equal(isSystemEndpointName('NVIDIA High Definition Audio'), true);
  assert.equal(isSystemEndpointName('ELAN Precision Touchpad'), true);
  assert.equal(isSystemEndpointName('Generic USB Hub'), true);

  assert.equal(isSystemEndpointName('Inphic KP 8K', 'Inphic'), false);
  assert.equal(isSystemEndpointName('凌豹 K98PRO 5.0', 'Sinowealth'), false);
  assert.equal(isSystemEndpointName('漫步者 HECATE G5 Max', 'Edifier'), false);
});

test('classifyKnownPeripheral recognizes Inphic KP 8K, K98PRO 5.0, and Edifier HECATE G5 Max', () => {
  const inphic = classifyKnownPeripheral('0x1D57', '0xFA65', 'Inphic KP 8K');
  assert.equal(inphic.category, 'mouse');
  assert.equal(inphic.maxPollingHz, 8000);
  assert.equal(inphic.protocolSupport, 'native-battery-experimental-tuning');

  const k98 = classifyKnownPeripheral('0x258A', '0x0040', '1.0.0');
  assert.equal(k98.displayName, '凌豹 K98PRO 5.0');
  assert.equal(k98.category, 'keyboard');
  assert.equal(k98.protocolSupport, 'native-battery-readonly');

  const g5 = classifyKnownPeripheral('BT-PNP', 'BT-AUDIO', 'HECATE G5 Max');
  assert.equal(g5.displayName, '漫步者 HECATE G5 Max');
  assert.equal(g5.category, 'headset');
  assert.equal(g5.protocolSupport, 'system-apo');
});

test('generateEqualizerApoConfig emits valid GraphicEQ & Preamp lines', () => {
  const cfg = generateEqualizerApoConfig(
    BUILTIN_EQ_PRESETS[1].bands,
    BUILTIN_EQ_PRESETS[1].preampDb,
    '漫步者 HECATE G5 Max'
  );
  assert.ok(cfg.includes('Preamp: -1.5 dB'));
  assert.ok(cfg.includes('GraphicEQ: 31 -3.5;'));
  assert.ok(cfg.includes('16000 1.0'));
});

test('filterDevices filters by protocol and query across name and VID/PID', () => {
  const all = filterDevices(FIXTURE_DEVICES, 'all', '');
  assert.equal(all.length, 3);

  const bt = filterDevices(FIXTURE_DEVICES, 'bluetooth', '');
  assert.equal(bt.length, 2);

  const byPid = filterDevices(FIXTURE_DEVICES, 'all', '0xFA65');
  assert.equal(byPid.length, 1);
  assert.equal(byPid[0].id, 'hw-0x1d57-0xfa65');

  const none = filterDevices(FIXTURE_DEVICES, 'usb', 'nonexistent-device');
  assert.equal(none.length, 0);
});

test('DPI clamping, stage updates, add and remove preserve valid invariants', () => {
  assert.equal(clampDpi(42), 100);
  assert.equal(clampDpi(50000), 30000);
  assert.equal(clampDpi(1624), 1600);
  assert.equal(clampDpi(1626), 1650);

  const synced = updateDpiStage(DEFAULT_MOUSE_CONFIG.dpiStages, 1, 2400, 1200, false);
  assert.equal(synced[1].dpiX, 2400);
  assert.equal(synced[1].dpiY, 2400);

  const split = updateDpiStage(DEFAULT_MOUSE_CONFIG.dpiStages, 1, 2400, 1200, true);
  assert.equal(split[1].dpiX, 2400);
  assert.equal(split[1].dpiY, 1200);

  const added = addDpiStage(DEFAULT_MOUSE_CONFIG.dpiStages);
  assert.equal(added.length, 5);

  const removed = removeDpiStage(added, 4, 4);
  assert.equal(removed.stages.length, 4);
  assert.equal(removed.activeIndex, 3);

  const single = removeDpiStage([added[0]], 0, 0);
  assert.equal(single.stages.length, 1);
});

test('buildHidFeatureReport produces 16-byte report with valid XOR checksum', () => {
  const report = buildHidFeatureReport(0x21, [0x06, 0x40, 0x0f, 0xa0]);
  assert.equal(report.bytes.length, 16);
  assert.equal(report.bytes[0], 0x08);
  assert.equal(report.bytes[1], 0x21);
  assert.equal(report.bytes[2], 4);

  let xor = 0;
  for (let i = 0; i < 15; i++) xor ^= report.bytes[i];
  assert.equal(report.checksum, xor);
  assert.equal(report.bytes[15], xor);
  assert.equal(report.hex.split(' ').length, 16);
});

test('validateActuation enforces physical bounds and RT <= actuation', () => {
  const clampedLow = validateActuation(0.02, 0.5);
  assert.equal(clampedLow.actuationMm, 0.1);
  assert.equal(clampedLow.rapidTriggerMm, 0.1);

  const normal = validateActuation(1.5, 0.25);
  assert.equal(normal.actuationMm, 1.5);
  assert.equal(normal.rapidTriggerMm, 0.25);
});

test('macro duration calculation, delay normalization, and name validation', () => {
  const steps: MacroStep[] = [
    { id: '1', kind: 'key-down', keyLabel: 'Q', delayMs: 18 },
    { id: '2', kind: 'key-up', keyLabel: 'Q', delayMs: 24 },
    { id: '3', kind: 'key-down', keyLabel: 'Q', delayMs: 18 },
    { id: '4', kind: 'key-up', keyLabel: 'Q', delayMs: 0 },
  ];
  assert.equal(calculateMacroDuration(steps), 60);

  const fixed = normalizeMacroDelays(steps, 'fixed', 25);
  assert.equal(fixed[0].delayMs, 25);
  assert.equal(fixed[fixed.length - 1].delayMs, 0);
  assert.equal(calculateMacroDuration(fixed), 75);

  const scaled = normalizeMacroDelays(steps, 'scale', 2);
  assert.equal(scaled[0].delayMs, 36);

  assert.equal(validateMacroName('   ', ['Macro A']), '请输入宏名称');
  assert.equal(validateMacroName('Macro A', ['Macro A']), '已存在同名宏，请更换名称');
  assert.equal(validateMacroName('Macro A', ['Macro A'], 'Macro A'), null);
  assert.equal(validateMacroName('新宏方案', ['Macro A']), null);
});

test('audio EQ preset lookup, gain clamping, SVG curve generation, and spatial positioning', () => {
  assert.equal(clampEqGain(20), 12);
  assert.equal(clampEqGain(-18), -12);

  const preset = applyEqPreset(BUILTIN_EQ_PRESETS, 'fps-footsteps');
  assert.ok(preset);
  assert.equal(preset.bands.length, 10);
  assert.equal(preset.preampDb, -1.5);

  const curve = generateEqCurvePath(preset.bands, 600, 200);
  assert.equal(curve.points.length, 10);
  assert.ok(curve.linePath.startsWith('M '));
  assert.ok(curve.areaPath.endsWith(' Z'));

  const centerNode = calculateSpatialNodePosition(
    DEFAULT_AUDIO_CONFIG.spatialChannels[1],
    80,
    120,
    120,
    90
  );
  assert.equal(centerNode.x, 120);
  assert.ok(centerNode.y < 120);
});

test('profile JSON export and import validates schema cleanly', () => {
  const json = serializeProfile({
    schemaVersion: '2.0.0',
    exportedAt: '2026-11-21T00:00:00.000Z',
    activeDeviceId: FIXTURE_DEVICES[0].id,
    devices: FIXTURE_DEVICES,
    mouseConfig: DEFAULT_MOUSE_CONFIG,
    keyboardConfig: DEFAULT_KEYBOARD_CONFIG,
    macros: [],
    audioConfig: DEFAULT_AUDIO_CONFIG,
  });

  const valid = parseAndValidateProfile(json);
  assert.equal(valid.ok, true);

  const invalid = parseAndValidateProfile('{"schemaVersion":"0.1"}');
  assert.equal(invalid.ok, false);

  const broken = parseAndValidateProfile('not-json');
  assert.equal(broken.ok, false);
});
