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
  filterDevices,
  generateEqCurvePath,
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
  INITIAL_AUDIO_CONFIG,
  INITIAL_DEVICES,
  INITIAL_KEYBOARD_CONFIG,
  INITIAL_MACROS,
  INITIAL_MOUSE_CONFIG,
} from '../data/seedDevices';

test('filterDevices filters by protocol and query across name and VID/PID', () => {
  const all = filterDevices(INITIAL_DEVICES, 'all', '');
  assert.equal(all.length, 3);

  const bt = filterDevices(INITIAL_DEVICES, 'bluetooth', '');
  assert.equal(bt.length, 3); // all 3 seed devices support tri-mode Bluetooth

  const byPid = filterDevices(INITIAL_DEVICES, 'all', '0xF51A');
  assert.equal(byPid.length, 1);
  assert.equal(byPid[0].id, 'dev-mouse-air-pro');

  const none = filterDevices(INITIAL_DEVICES, 'usb', 'nonexistent-device');
  assert.equal(none.length, 0);
});

test('DPI clamping, stage updates, add and remove preserve valid invariants', () => {
  assert.equal(clampDpi(42), 100);
  assert.equal(clampDpi(50000), 30000);
  assert.equal(clampDpi(1624), 1600);
  assert.equal(clampDpi(1626), 1650);

  const synced = updateDpiStage(INITIAL_MOUSE_CONFIG.dpiStages, 1, 2400, 1200, false);
  assert.equal(synced[1].dpiX, 2400);
  assert.equal(synced[1].dpiY, 2400);

  const split = updateDpiStage(INITIAL_MOUSE_CONFIG.dpiStages, 1, 2400, 1200, true);
  assert.equal(split[1].dpiX, 2400);
  assert.equal(split[1].dpiY, 1200);

  const added = addDpiStage(INITIAL_MOUSE_CONFIG.dpiStages);
  assert.equal(added.length, 5);

  const removed = removeDpiStage(added, 4, 4);
  assert.equal(removed.stages.length, 4);
  assert.equal(removed.activeIndex, 3);

  const single = removeDpiStage([added[0]], 0, 0);
  assert.equal(single.stages.length, 1); // never removes last stage
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
  const steps = INITIAL_MACROS[0].steps;
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
    INITIAL_AUDIO_CONFIG.spatialChannels[1], // C at 0 deg
    80,
    120,
    120,
    90
  );
  assert.equal(centerNode.x, 120);
  assert.ok(centerNode.y < 120); // Front center is directly above origin
});

test('profile JSON export and import validates schema cleanly', () => {
  const json = serializeProfile({
    schemaVersion: '1.0.0',
    exportedAt: '2026-11-21T00:00:00.000Z',
    activeDeviceId: INITIAL_DEVICES[0].id,
    devices: INITIAL_DEVICES,
    mouseConfig: INITIAL_MOUSE_CONFIG,
    keyboardConfig: INITIAL_KEYBOARD_CONFIG,
    macros: INITIAL_MACROS,
    audioConfig: INITIAL_AUDIO_CONFIG,
  });

  const valid = parseAndValidateProfile(json);
  assert.equal(valid.ok, true);

  const invalid = parseAndValidateProfile('{"schemaVersion":"0.1"}');
  assert.equal(invalid.ok, false);

  const broken = parseAndValidateProfile('not-json');
  assert.equal(broken.ok, false);
});
