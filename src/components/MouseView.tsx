import { useState } from 'react';
import { ChevronDown, ChevronUp, Plus, RotateCcw, Trash2 } from 'lucide-react';
import type { KeyBinding, MacroDefinition, MouseConfig, PeripheralDevice } from '../types/peripheral';
import {
  addDpiStage,
  buildHidFeatureReport,
  DPI_MAX,
  DPI_MIN,
  DPI_STEP,
  removeDpiStage,
  updateDpiStage,
} from '../utils/deviceEngine';

interface MouseViewProps {
  device: PeripheralDevice | null;
  config: MouseConfig;
  macros: MacroDefinition[];
  onChange: (next: MouseConfig) => void;
  onReset: () => void;
}

const POLLING_RATES: MouseConfig['pollingRateHz'][] = [125, 250, 500, 1000, 2000, 4000, 8000];
const LOD_OPTIONS: MouseConfig['liftOffDistanceMm'][] = [0.7, 1.0, 2.0];

const MOUSE_BUTTON_META: { id: string; name: string; defaultLabel: string; cx: number; cy: number }[] = [
  { id: 'left', name: '左键', defaultLabel: '鼠标左键', cx: 88, cy: 96 },
  { id: 'right', name: '右键', defaultLabel: '鼠标右键', cx: 172, cy: 96 },
  { id: 'middle', name: '中键滚轮', defaultLabel: '中键点击', cx: 130, cy: 90 },
  { id: 'dpiBtn', name: 'DPI 切换键', defaultLabel: '循环切换 DPI', cx: 130, cy: 144 },
  { id: 'sideForward', name: '侧键前 (M5)', defaultLabel: '浏览器前进', cx: 58, cy: 158 },
  { id: 'sideBack', name: '侧键后 (M4)', defaultLabel: '浏览器后退', cx: 58, cy: 204 },
];

const QUICK_BINDINGS: { group: string; items: KeyBinding[] }[] = [
  {
    group: '基础鼠标动作',
    items: [
      { type: 'default', value: 'MouseLeft', label: '鼠标左键' },
      { type: 'default', value: 'MouseRight', label: '鼠标右键' },
      { type: 'default', value: 'MouseMiddle', label: '中键点击' },
      { type: 'mouse', value: 'BrowserForward', label: '浏览器前进' },
      { type: 'mouse', value: 'BrowserBack', label: '浏览器后退' },
      { type: 'dpi-cycle', value: 'DpiCycle', label: '循环切换 DPI' },
    ],
  },
  {
    group: '系统与多媒体',
    items: [
      { type: 'combo', value: 'Ctrl+C', label: '复制 (Ctrl+C)' },
      { type: 'combo', value: 'Ctrl+V', label: '粘贴 (Ctrl+V)' },
      { type: 'media', value: 'MediaPlayPause', label: '播放 / 暂停' },
      { type: 'media', value: 'VolumeMute', label: '系统静音' },
      { type: 'disabled', value: 'None', label: '禁用此按键' },
    ],
  },
];

export function MouseView({ device, config, macros, onChange, onReset }: MouseViewProps) {
  const [selectedButtonId, setSelectedButtonId] = useState<string>('sideForward');
  const [showHidHex, setShowHidHex] = useState(false);

  const activeStage = config.dpiStages[config.activeStageIndex] ?? config.dpiStages[0];
  const selectedButtonMeta =
    MOUSE_BUTTON_META.find((b) => b.id === selectedButtonId) ?? MOUSE_BUTTON_META[0];
  const currentBinding = config.buttonBindings[selectedButtonId] ?? {
    type: 'default',
    value: selectedButtonMeta.id,
    label: selectedButtonMeta.defaultLabel,
  };

  // Real 16-byte HID Feature Report preview for current DPI & Polling Rate
  const hidPacket = buildHidFeatureReport(0x21, [
    config.activeStageIndex,
    (activeStage.dpiX >> 8) & 0xff,
    activeStage.dpiX & 0xff,
    (config.pollingRateHz >> 8) & 0xff,
    config.pollingRateHz & 0xff,
    Math.round(config.liftOffDistanceMm * 10),
    config.debounceMs,
    (config.motionSync ? 1 : 0) | (config.angleSnapping ? 2 : 0) | (config.rippleControl ? 4 : 0),
  ]);

  const handleAssignBinding = (binding: KeyBinding) => {
    onChange({
      ...config,
      buttonBindings: {
        ...config.buttonBindings,
        [selectedButtonId]: binding,
      },
    });
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">
            鼠标调校与按键映射
          </h1>
          <p className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[#586370]">
            <span>{device ? device.name : '未连接实体鼠标（本地离线预设编辑）'}</span>
            {device && (
              <>
                <span aria-hidden="true">·</span>
                <span className="font-mono tabular-nums">
                  {device.vendorId}:{device.productId}
                </span>
                {device.batteryPercent !== null && (
                  <>
                    <span aria-hidden="true">·</span>
                    <span className="font-mono font-medium tabular-nums text-[#2B6B68]">
                      电量 {device.batteryPercent}%
                    </span>
                  </>
                )}
              </>
            )}
            <span aria-hidden="true">·</span>
            <span className="font-mono tabular-nums">
              当前 {activeStage.dpiX} DPI / {config.pollingRateHz} Hz
            </span>
          </p>
        </div>

        <button
          type="button"
          onClick={onReset}
          className="inline-flex items-center gap-1.5 self-start rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] transition-colors whitespace-nowrap"
        >
          <RotateCcw className="h-3.5 w-3.5" />
          <span>恢复默认参数</span>
        </button>
      </div>

      {device?.protocolNote && (
        <div className="rounded-xl border border-[#DCE5E4] bg-[#F4F8F7] px-4 py-2.5 text-xs leading-relaxed text-[#1D4E4C]">
          {device.protocolNote}
        </div>
      )}

      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        {/* Left: Interactive Mouse Blueprint & Key Binding */}
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 xl:col-span-5 flex flex-col">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-[#181C20]">物理按键定义</h2>
            <span className="text-xs text-[#84909D]">点击示意图节点重映射</span>
          </div>

          <div className="mt-4 flex flex-col items-center">
            <svg
              viewBox="0 0 260 320"
              className="w-full max-w-[230px] select-none"
              role="img"
              aria-label="鼠标顶部按键交互示意图"
            >
              {/* Subtle minimalist mouse outline */}
              <path
                d="M130 24 C84 24, 54 62, 54 126 L54 206 C54 268, 86 298, 130 298 C174 298, 206 268, 206 206 L206 126 C206 62, 176 24, 130 24 Z"
                fill="#F7F8FA"
                stroke="#CFD7E0"
                strokeWidth="2"
              />
              {/* Left/Right click split */}
              <path d="M56 126 L204 126" stroke="#DCE2E8" strokeWidth="1.5" />
              <path d="M130 24 L130 126" stroke="#DCE2E8" strokeWidth="1.5" />
              {/* Scroll wheel */}
              <rect
                x="120"
                y="62"
                width="20"
                height="48"
                rx="10"
                fill="#FFFFFF"
                stroke="#B8C3CF"
                strokeWidth="1.5"
              />
              {/* Side buttons */}
              <rect x="48" y="144" width="6" height="28" rx="3" fill="#B8C3CF" />
              <rect x="48" y="190" width="6" height="28" rx="3" fill="#B8C3CF" />

              {/* Interactive hotspot nodes */}
              {MOUSE_BUTTON_META.map((btn) => {
                const selected = btn.id === selectedButtonId;
                return (
                  <g
                    key={btn.id}
                    onClick={() => setSelectedButtonId(btn.id)}
                    className="cursor-pointer"
                  >
                    <circle
                      cx={btn.cx}
                      cy={btn.cy}
                      r={selected ? 13 : 10}
                      fill={selected ? '#2B6B68' : '#FFFFFF'}
                      stroke={selected ? '#2B6B68' : '#84909D'}
                      strokeWidth="2"
                    />
                    <circle
                      cx={btn.cx}
                      cy={btn.cy}
                      r="3.5"
                      fill={selected ? '#FFFFFF' : '#2B6B68'}
                    />
                  </g>
                );
              })}
            </svg>

            {/* Accessible native button selector synced with SVG */}
            <div className="mt-4 grid w-full grid-cols-2 gap-2">
              {MOUSE_BUTTON_META.map((btn) => {
                const selected = btn.id === selectedButtonId;
                const bound = config.buttonBindings[btn.id];
                return (
                  <button
                    key={btn.id}
                    type="button"
                    onClick={() => setSelectedButtonId(btn.id)}
                    className={`rounded-xl border px-3 py-2 text-left transition-colors ${
                      selected
                        ? 'border-[#2B6B68] bg-[#EBF3F2]'
                        : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#C9D3DC]'
                    }`}
                  >
                    <div className="text-[11px] text-[#586370]">{btn.name}</div>
                    <div className="mt-0.5 truncate text-xs font-medium text-[#181C20]">
                      {bound?.label ?? btn.defaultLabel}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Binding Editor */}
          <div className="mt-5 border-t border-[#EEF1F4] pt-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-[#181C20]">
                为「{selectedButtonMeta.name}」分配功能
              </span>
              <span className="text-xs text-[#2B6B68] font-medium">{currentBinding.label}</span>
            </div>

            {QUICK_BINDINGS.map((group) => (
              <div key={group.group}>
                <div className="mb-1.5 text-[11px] text-[#84909D]">{group.group}</div>
                <div className="flex flex-wrap gap-1.5">
                  {group.items.map((item) => {
                    const isCurrent =
                      currentBinding.value === item.value && currentBinding.type === item.type;
                    return (
                      <button
                        key={item.value}
                        type="button"
                        onClick={() => handleAssignBinding(item)}
                        className={`rounded-lg px-2.5 py-1.5 text-xs transition-colors whitespace-nowrap ${
                          isCurrent
                            ? 'bg-[#2B6B68] text-white'
                            : 'bg-[#F0F3F6] text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20]'
                        }`}
                      >
                        {item.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}

            <div>
              <div className="mb-1.5 text-[11px] text-[#84909D]">绑定已保存宏方案</div>
              {macros.length === 0 ? (
                <p className="text-xs text-[#84909D]">暂无宏，请前往“宏工作台”创建。</p>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {macros.map((m) => {
                    const isBound =
                      currentBinding.type === 'macro' && currentBinding.macroId === m.id;
                    return (
                      <button
                        key={m.id}
                        type="button"
                        onClick={() =>
                          handleAssignBinding({
                            type: 'macro',
                            value: m.id,
                            label: `宏 · ${m.name}`,
                            macroId: m.id,
                          })
                        }
                        className={`rounded-lg px-2.5 py-1.5 text-xs transition-colors whitespace-nowrap ${
                          isBound
                            ? 'bg-[#2B6B68] text-white'
                            : 'bg-[#F0F3F6] text-[#181C20] hover:bg-[#EBF3F2] hover:text-[#1D4E4C]'
                        }`}
                      >
                        {m.name}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </section>

        {/* Right: DPI Matrix + Polling Rate + Sensor Tuning */}
        <div className="space-y-6 xl:col-span-7">
          {/* DPI Editor */}
          <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="text-sm font-semibold text-[#181C20]">DPI 灵敏度档位</h2>
                <p className="mt-0.5 text-xs text-[#586370]">
                  范围 100 – 30,000 DPI，支持 50 DPI 精细步进与 X/Y 独立校准。
                </p>
              </div>
              <div className="flex items-center gap-3">
                <label className="inline-flex items-center gap-2 text-xs text-[#586370] cursor-pointer">
                  <input
                    type="checkbox"
                    checked={config.independentXY}
                    onChange={(e) =>
                      onChange({
                        ...config,
                        independentXY: e.target.checked,
                      })
                    }
                    className="h-4 w-4 rounded border-[#CBD3DC] accent-[#2B6B68]"
                  />
                  <span>独立 X/Y 轴</span>
                </label>
                <button
                  type="button"
                  disabled={config.dpiStages.length >= 6}
                  onClick={() =>
                    onChange({
                      ...config,
                      dpiStages: addDpiStage(config.dpiStages),
                    })
                  }
                  className="inline-flex items-center gap-1 rounded-lg border border-[#E5E9EE] px-2.5 py-1.5 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] disabled:opacity-40 whitespace-nowrap"
                >
                  <Plus className="h-3.5 w-3.5" />
                  <span>新增档位</span>
                </button>
              </div>
            </div>

            {/* Stage selector tabs */}
            <div className="mt-4 grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              {config.dpiStages.map((stage, idx) => {
                const active = idx === config.activeStageIndex;
                return (
                  <div
                    key={stage.id}
                    className={`flex items-center justify-between rounded-xl border px-3 py-2.5 transition-colors ${
                      active
                        ? 'border-[#2B6B68] bg-[#EBF3F2]'
                        : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#C9D3DC]'
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => onChange({ ...config, activeStageIndex: idx })}
                      className="flex-1 text-left"
                    >
                      <div className="text-[11px] text-[#586370]">档位 {idx + 1}</div>
                      <div className="mt-0.5 font-mono text-sm font-semibold tabular-nums text-[#181C20]">
                        {stage.dpiX}
                        {config.independentXY && stage.dpiY !== stage.dpiX ? ` / ${stage.dpiY}` : ''}
                      </div>
                    </button>
                    {config.dpiStages.length > 1 && (
                      <button
                        type="button"
                        aria-label={`删除 DPI 档位 ${idx + 1}`}
                        onClick={() => {
                          const res = removeDpiStage(config.dpiStages, idx, config.activeStageIndex);
                          onChange({
                            ...config,
                            dpiStages: res.stages,
                            activeStageIndex: res.activeIndex,
                          });
                        }}
                        className="rounded-md p-1 text-[#84909D] hover:bg-white hover:text-[#B91C1C]"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Active stage sliders */}
            <div className="mt-5 space-y-4">
              <div>
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="dpi-x-slider" className="font-medium text-[#181C20]">
                    {config.independentXY ? 'X 轴灵敏度 (横向)' : '当前档位 DPI'}
                  </label>
                  <span className="font-mono text-sm font-semibold tabular-nums text-[#2B6B68]">
                    {activeStage.dpiX} DPI
                  </span>
                </div>
                <input
                  id="dpi-x-slider"
                  type="range"
                  min={DPI_MIN}
                  max={DPI_MAX}
                  step={DPI_STEP}
                  value={activeStage.dpiX}
                  onChange={(e) =>
                    onChange({
                      ...config,
                      dpiStages: updateDpiStage(
                        config.dpiStages,
                        config.activeStageIndex,
                        Number(e.target.value),
                        activeStage.dpiY,
                        config.independentXY
                      ),
                    })
                  }
                  className="lumen-slider mt-2 w-full"
                />
                <div className="mt-1 flex justify-between font-mono text-[11px] tabular-nums text-[#84909D]">
                  <span>100</span>
                  <span>800</span>
                  <span>1600</span>
                  <span>3200</span>
                  <span>30000</span>
                </div>
              </div>

              {config.independentXY && (
                <div>
                  <div className="flex items-center justify-between text-xs">
                    <label htmlFor="dpi-y-slider" className="font-medium text-[#181C20]">
                      Y 轴灵敏度 (纵向)
                    </label>
                    <span className="font-mono text-sm font-semibold tabular-nums text-[#2B6B68]">
                      {activeStage.dpiY} DPI
                    </span>
                  </div>
                  <input
                    id="dpi-y-slider"
                    type="range"
                    min={DPI_MIN}
                    max={DPI_MAX}
                    step={DPI_STEP}
                    value={activeStage.dpiY}
                    onChange={(e) =>
                      onChange({
                        ...config,
                        dpiStages: updateDpiStage(
                          config.dpiStages,
                          config.activeStageIndex,
                          activeStage.dpiX,
                          Number(e.target.value),
                          true
                        ),
                      })
                    }
                    className="lumen-slider mt-2 w-full"
                  />
                </div>
              )}
            </div>
          </section>

          {/* Polling Rate & Sensor Parameters */}
          <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 space-y-6">
            <div>
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold text-[#181C20]">回报率 (Polling Rate)</h2>
                <span className="font-mono text-xs tabular-nums text-[#586370]">
                  采样间隔 {(1000 / config.pollingRateHz).toFixed(3)} ms
                </span>
              </div>
              <div className="mt-3 grid grid-cols-4 gap-2 sm:grid-cols-7">
                {POLLING_RATES.map((rate) => {
                  const active = config.pollingRateHz === rate;
                  return (
                    <button
                      key={rate}
                      type="button"
                      onClick={() => onChange({ ...config, pollingRateHz: rate })}
                      className={`rounded-lg py-2 font-mono text-xs font-medium tabular-nums transition-colors whitespace-nowrap ${
                        active
                          ? 'bg-[#2B6B68] text-white'
                          : 'bg-[#F4F6F8] text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20]'
                      }`}
                    >
                      {rate >= 1000 ? `${rate / 1000}K` : rate}
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="grid grid-cols-1 gap-5 border-t border-[#EEF1F4] pt-5 sm:grid-cols-2">
              <div>
                <div className="text-xs font-medium text-[#181C20]">静默高度 (LOD)</div>
                <div className="mt-2 flex gap-2">
                  {LOD_OPTIONS.map((lod) => {
                    const active = config.liftOffDistanceMm === lod;
                    return (
                      <button
                        key={lod}
                        type="button"
                        onClick={() => onChange({ ...config, liftOffDistanceMm: lod })}
                        className={`flex-1 rounded-lg py-1.5 font-mono text-xs font-medium tabular-nums transition-colors ${
                          active
                            ? 'bg-[#181C20] text-white'
                            : 'bg-[#F4F6F8] text-[#586370] hover:bg-[#E5E9EE]'
                        }`}
                      >
                        {lod.toFixed(1)} mm
                      </button>
                    );
                  })}
                </div>
              </div>

              <div>
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="mouse-debounce" className="font-medium text-[#181C20]">
                    微动去抖延迟 (Debounce)
                  </label>
                  <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                    {config.debounceMs} ms
                  </span>
                </div>
                <input
                  id="mouse-debounce"
                  type="range"
                  min={0}
                  max={16}
                  step={1}
                  value={config.debounceMs}
                  onChange={(e) => onChange({ ...config, debounceMs: Number(e.target.value) })}
                  className="lumen-slider mt-3 w-full"
                />
              </div>
            </div>

            <div className="grid grid-cols-1 gap-3 border-t border-[#EEF1F4] pt-5 sm:grid-cols-3">
              {[
                {
                  key: 'motionSync' as const,
                  title: 'Motion Sync',
                  desc: '同步传感器信号与 USB 帧周期',
                },
                {
                  key: 'angleSnapping' as const,
                  title: '直线修正',
                  desc: '平滑水平快速拉枪轨迹抖动',
                },
                {
                  key: 'rippleControl' as const,
                  title: '波纹控制',
                  desc: '抑制高 DPI 下的微小锯齿抖动',
                },
              ].map((item) => {
                const enabled = config[item.key];
                return (
                  <button
                    key={item.key}
                    type="button"
                    role="switch"
                    aria-checked={enabled}
                    onClick={() => onChange({ ...config, [item.key]: !enabled })}
                    className={`rounded-xl border p-3.5 text-left transition-colors ${
                      enabled
                        ? 'border-[#C4DCD9] bg-[#EBF3F2]/70'
                        : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#C9D3DC]'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-semibold text-[#181C20]">{item.title}</span>
                      <span
                        className={`text-[11px] font-medium ${
                          enabled ? 'text-[#1D4E4C]' : 'text-[#84909D]'
                        }`}
                      >
                        {enabled ? '已开启' : '已关闭'}
                      </span>
                    </div>
                    <p className="mt-1 text-[11px] leading-relaxed text-[#586370]">{item.desc}</p>
                  </button>
                );
              })}
            </div>

            {/* Collapsible HID Feature Report Inspector */}
            <div className="border-t border-[#EEF1F4] pt-4">
              <button
                type="button"
                aria-expanded={showHidHex}
                onClick={() => setShowHidHex((v) => !v)}
                className="flex w-full items-center justify-between text-xs font-medium text-[#586370] hover:text-[#181C20]"
              >
                <span>HID Feature Report 16 字节下发帧预览</span>
                {showHidHex ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
              </button>
              {showHidHex && (
                <div className="mt-3 rounded-xl bg-[#F4F6F8] px-4 py-3 font-mono text-xs text-[#181C20] overflow-x-auto">
                  <div className="text-[11px] text-[#586370] mb-1">
                    CMD 0x21 · Report ID 0x08 · XOR 校验 0x
                    {hidPacket.checksum.toString(16).toUpperCase().padStart(2, '0')}
                  </div>
                  <div className="tabular-nums tracking-wider">{hidPacket.hex}</div>
                </div>
              )}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
