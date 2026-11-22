import type {
  AppProfileSnapshot,
  AudioPreset,
  DpiStage,
  MacroStep,
  PeripheralDevice,
  ProtocolFilter,
  SpatialChannel,
} from '../types/peripheral';

export const DPI_MIN = 100;
export const DPI_MAX = 30000;
export const DPI_STEP = 50;
export const EQ_MIN_DB = -12;
export const EQ_MAX_DB = 12;

export function filterDevices(
  devices: PeripheralDevice[],
  protocol: ProtocolFilter,
  searchQuery: string
): PeripheralDevice[] {
  const q = searchQuery.trim().toLowerCase();
  return devices.filter((dev) => {
    const matchesProtocol =
      protocol === 'all' ||
      dev.activeProtocol === protocol ||
      dev.supportedProtocols.includes(protocol);

    if (!matchesProtocol) return false;
    if (!q) return true;

    return (
      dev.name.toLowerCase().includes(q) ||
      dev.model.toLowerCase().includes(q) ||
      dev.vendorId.toLowerCase().includes(q) ||
      dev.productId.toLowerCase().includes(q)
    );
  });
}

export function clampDpi(value: number): number {
  if (!Number.isFinite(value)) return 800;
  const clamped = Math.min(DPI_MAX, Math.max(DPI_MIN, value));
  return Math.round(clamped / DPI_STEP) * DPI_STEP;
}

export function updateDpiStage(
  stages: DpiStage[],
  index: number,
  dpiX: number,
  dpiY: number,
  independentXY: boolean
): DpiStage[] {
  const nextX = clampDpi(dpiX);
  const nextY = independentXY ? clampDpi(dpiY) : nextX;
  return stages.map((stage, i) =>
    i === index ? { ...stage, dpiX: nextX, dpiY: nextY } : stage
  );
}

export function addDpiStage(stages: DpiStage[]): DpiStage[] {
  if (stages.length >= 6) return stages;
  const palette = ['#64748B', '#2B6B68', '#4A7C9B', '#8A6F52', '#6B5B8A', '#4C7A5B'];
  const last = stages[stages.length - 1];
  const nextDpi = clampDpi((last?.dpiX ?? 800) + 800);
  return [
    ...stages,
    {
      id: `dpi-${Date.now()}-${stages.length + 1}`,
      dpiX: nextDpi,
      dpiY: nextDpi,
      color: palette[stages.length % palette.length],
    },
  ];
}

export function removeDpiStage(
  stages: DpiStage[],
  index: number,
  activeIndex: number
): { stages: DpiStage[]; activeIndex: number } {
  if (stages.length <= 1) {
    return { stages, activeIndex };
  }
  const nextStages = stages.filter((_, i) => i !== index);
  const nextActive = Math.min(activeIndex, nextStages.length - 1);
  return { stages: nextStages, activeIndex: Math.max(0, nextActive) };
}

/**
 * Generates a 16-byte HID Feature Report preview (standardized LumenHID protocol v1)
 * Byte 0: Report ID (0x08)
 * Byte 1: Command ID
 * Byte 2: Payload length
 * Bytes 3..14: Payload bytes
 * Byte 15: XOR Checksum of bytes 0..14
 */
export function buildHidFeatureReport(commandId: number, payload: number[]): {
  bytes: number[];
  hex: string;
  checksum: number;
} {
  const packet = new Array<number>(16).fill(0);
  packet[0] = 0x08;
  packet[1] = commandId & 0xff;
  const len = Math.min(12, payload.length);
  packet[2] = len;
  for (let i = 0; i < len; i++) {
    packet[3 + i] = payload[i] & 0xff;
  }
  let checksum = 0;
  for (let i = 0; i < 15; i++) {
    checksum ^= packet[i];
  }
  packet[15] = checksum & 0xff;

  const hex = packet.map((b) => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
  return { bytes: packet, hex, checksum: packet[15] };
}

export function validateActuation(
  actuationMm: number,
  rapidTriggerMm: number
): { actuationMm: number; rapidTriggerMm: number } {
  const safeActuation = Number(Math.min(4.0, Math.max(0.1, actuationMm)).toFixed(2));
  const maxRt = Math.min(2.0, safeActuation);
  const safeRt = Number(Math.min(maxRt, Math.max(0.05, rapidTriggerMm)).toFixed(2));
  return { actuationMm: safeActuation, rapidTriggerMm: safeRt };
}

export function calculateMacroDuration(steps: MacroStep[]): number {
  return steps.reduce((sum, step) => sum + Math.max(0, Math.round(step.delayMs)), 0);
}

export function normalizeMacroDelays(
  steps: MacroStep[],
  mode: 'fixed' | 'scale' | 'zero',
  value: number
): MacroStep[] {
  return steps.map((step, idx) => {
    if (idx === steps.length - 1 && step.kind !== 'delay') {
      return { ...step, delayMs: 0 };
    }
    if (mode === 'zero') {
      return { ...step, delayMs: 5 };
    }
    if (mode === 'fixed') {
      return { ...step, delayMs: Math.min(5000, Math.max(5, Math.round(value))) };
    }
    const scaled = Math.round(step.delayMs * value);
    return { ...step, delayMs: Math.min(5000, Math.max(5, scaled)) };
  });
}

export function validateMacroName(name: string, existingNames: string[], currentName?: string): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    return '请输入宏名称';
  }
  if (trimmed.length > 32) {
    return '宏名称长度不能超过 32 个字符';
  }
  const lower = trimmed.toLowerCase();
  const conflict = existingNames.some(
    (n) => n.toLowerCase() === lower && (!currentName || n.toLowerCase() !== currentName.toLowerCase())
  );
  if (conflict) {
    return '已存在同名宏，请更换名称';
  }
  return null;
}

export function clampEqGain(db: number): number {
  if (!Number.isFinite(db)) return 0;
  return Number(Math.min(EQ_MAX_DB, Math.max(EQ_MIN_DB, db)).toFixed(1));
}

export function applyEqPreset(presets: AudioPreset[], presetId: string): {
  bands: number[];
  preampDb: number;
} | null {
  const found = presets.find((p) => p.id === presetId);
  if (!found) return null;
  return {
    bands: found.bands.map(clampEqGain),
    preampDb: Number(Math.min(6, Math.max(-6, found.preampDb)).toFixed(1)),
  };
}

/**
 * Computes a smooth SVG cubic Bezier path for a 10-band EQ curve.
 */
export function generateEqCurvePath(
  bands: number[],
  width: number,
  height: number,
  paddingX = 24,
  paddingY = 20
): { linePath: string; areaPath: string; points: { x: number; y: number; db: number }[] } {
  const count = bands.length;
  if (count === 0) {
    return { linePath: '', areaPath: '', points: [] };
  }
  const usableW = width - paddingX * 2;
  const usableH = height - paddingY * 2;
  const midY = paddingY + usableH / 2;

  const points = bands.map((rawDb, i) => {
    const db = clampEqGain(rawDb);
    const x = count === 1 ? width / 2 : paddingX + (i / (count - 1)) * usableW;
    const y = midY - (db / EQ_MAX_DB) * (usableH / 2);
    return { x: Number(x.toFixed(1)), y: Number(y.toFixed(1)), db };
  });

  if (points.length === 1) {
    const p = points[0];
    return {
      linePath: `M ${p.x} ${p.y}`,
      areaPath: `M ${p.x} ${midY} L ${p.x} ${p.y} L ${p.x} ${midY} Z`,
      points,
    };
  }

  let linePath = `M ${points[0].x} ${points[0].y}`;
  for (let i = 0; i < points.length - 1; i++) {
    const p0 = points[Math.max(0, i - 1)];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[Math.min(points.length - 1, i + 2)];

    const cp1x = Number((p1.x + (p2.x - p0.x) / 6).toFixed(1));
    const cp1y = Number((p1.y + (p2.y - p0.y) / 6).toFixed(1));
    const cp2x = Number((p2.x - (p3.x - p1.x) / 6).toFixed(1));
    const cp2y = Number((p2.y - (p3.y - p1.y) / 6).toFixed(1));

    linePath += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${p2.x} ${p2.y}`;
  }

  const last = points[points.length - 1];
  const areaPath = `${linePath} L ${last.x} ${midY} L ${points[0].x} ${midY} Z`;
  return { linePath, areaPath, points };
}

/**
 * Calculates virtual 7.1 speaker polar coordinates & effective perceptual gain
 */
export function calculateSpatialNodePosition(
  channel: SpatialChannel,
  soundstageWidth: number,
  centerX: number,
  centerY: number,
  maxRadius: number
): { x: number; y: number } {
  const widthFactor = 0.55 + (Math.min(100, Math.max(0, soundstageWidth)) / 100) * 0.45;
  const gainNormalized = (clampEqGain(channel.gainDb) + EQ_MAX_DB) / (EQ_MAX_DB * 2); // 0..1
  // Higher gain brings perceived acoustic source closer to listener
  const distanceFactor = (1.0 - gainNormalized * 0.34) * widthFactor;
  const radius = maxRadius * distanceFactor;
  const rad = ((channel.angleDeg - 90) * Math.PI) / 180;
  return {
    x: Number((centerX + Math.cos(rad) * radius).toFixed(1)),
    y: Number((centerY + Math.sin(rad) * radius).toFixed(1)),
  };
}

export function serializeProfile(snapshot: AppProfileSnapshot): string {
  return JSON.stringify(snapshot, null, 2);
}

export function parseAndValidateProfile(rawJson: string): {
  ok: true;
  data: AppProfileSnapshot;
} | {
  ok: false;
  error: string;
} {
  try {
    const parsed = JSON.parse(rawJson) as Partial<AppProfileSnapshot>;
    if (!parsed || typeof parsed !== 'object') {
      return { ok: false, error: '配置文件格式无效，需要 JSON 对象' };
    }
    if (parsed.schemaVersion !== '1.0.0') {
      return { ok: false, error: '不支持的配置文件版本号' };
    }
    if (!Array.isArray(parsed.devices) || !parsed.mouseConfig || !parsed.keyboardConfig || !parsed.audioConfig || !Array.isArray(parsed.macros)) {
      return { ok: false, error: '配置文件缺少必要的外设或音频字段' };
    }
    return { ok: true, data: parsed as AppProfileSnapshot };
  } catch {
    return { ok: false, error: 'JSON 解析失败，请检查文件内容是否完整' };
  }
}
