import { useState } from 'react';
import { RotateCcw } from 'lucide-react';
import { KEYBOARD_75_ROWS } from '../data/seedDevices';
import type {
  AmbientLightingMode,
  KeyBinding,
  KeyboardConfig,
  KeyboardLayerId,
  MacroDefinition,
  PeripheralDevice,
} from '../types/peripheral';
import { validateActuation } from '../utils/deviceEngine';

interface KeyboardViewProps {
  device: PeripheralDevice;
  config: KeyboardConfig;
  macros: MacroDefinition[];
  onChange: (next: KeyboardConfig) => void;
  onReset: () => void;
}

const LAYERS: { id: KeyboardLayerId; label: string }[] = [
  { id: 'base', label: '默认层 Base' },
  { id: 'fn1', label: '功能层 Fn1' },
  { id: 'game', label: '游戏层 Game' },
];

const LIGHTING_MODES: { id: AmbientLightingMode; label: string }[] = [
  { id: 'off', label: '关闭背光' },
  { id: 'static-mist', label: '雾白常亮' },
  { id: 'breathing', label: '柔光呼吸' },
  { id: 'reactive', label: '单点触发' },
];

const KEYBOARD_QUICK_REMAPS: KeyBinding[] = [
  { type: 'key', value: 'ControlLeft', label: '左 Ctrl' },
  { type: 'key', value: 'Escape', label: 'Esc' },
  { type: 'key', value: 'F13', label: '功能键 F13' },
  { type: 'key', value: 'F14', label: '功能键 F14' },
  { type: 'media', value: 'MediaPlayPause', label: '播放 / 暂停' },
  { type: 'media', value: 'VolumeMute', label: '静音' },
  { type: 'media', value: 'VolumeUp', label: '音量 +' },
  { type: 'media', value: 'VolumeDown', label: '音量 -' },
  { type: 'disabled', value: 'None', label: '已禁用' },
];

export function KeyboardView({ device, config, macros, onChange, onReset }: KeyboardViewProps) {
  const [selectedKeyCode, setSelectedKeyCode] = useState<string>('CapsLock');

  const currentLayerMap = config.layerBindings[config.activeLayer] ?? {};
  const selectedKeyDef =
    KEYBOARD_75_ROWS.flat().find((k) => k.code === selectedKeyCode) ?? KEYBOARD_75_ROWS[0][0];
  const selectedBinding = currentLayerMap[selectedKeyCode];

  const handleAssignKey = (binding: KeyBinding | null) => {
    const nextLayer = { ...currentLayerMap };
    if (binding === null) {
      delete nextLayer[selectedKeyCode];
    } else {
      nextLayer[selectedKeyCode] = binding;
    }
    onChange({
      ...config,
      layerBindings: {
        ...config.layerBindings,
        [config.activeLayer]: nextLayer,
      },
    });
  };

  const handleActuationChange = (actuationMm: number, rtMm: number) => {
    const safe = validateActuation(actuationMm, rtMm);
    onChange({
      ...config,
      globalActuationMm: safe.actuationMm,
      rapidTriggerSensitivityMm: safe.rapidTriggerMm,
    });
  };

  const actuationPercent = (config.globalActuationMm / 4.0) * 100;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">
            键盘布局与磁轴调校
          </h1>
          <p className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[#586370]">
            <span>{device.name}</span>
            <span aria-hidden="true">·</span>
            <span className="font-mono tabular-nums">触发行程 {config.globalActuationMm.toFixed(2)} mm</span>
            <span aria-hidden="true">·</span>
            <span>{config.rapidTriggerEnabled ? `RT ${config.rapidTriggerSensitivityMm.toFixed(2)} mm` : 'RT 关闭'}</span>
            <span aria-hidden="true">·</span>
            <span className="font-mono tabular-nums">{config.pollingRateHz} Hz</span>
          </p>
        </div>

        <button
          type="button"
          onClick={onReset}
          className="inline-flex items-center gap-1.5 self-start rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] transition-colors whitespace-nowrap"
        >
          <RotateCcw className="h-3.5 w-3.5" />
          <span>恢复默认键位</span>
        </button>
      </div>

      {/* 75% Interactive Keymap Surface */}
      <section className="rounded-2xl border border-[#E5E9EE] bg-white p-5 sm:p-6">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-sm font-semibold text-[#181C20]">75% 交互式按键定义</h2>
            <p className="mt-0.5 text-xs text-[#586370]">
              点选下方任意键帽，可独立配置三层映射、多媒体指令或一键绑定宏。
            </p>
          </div>

          <div
            role="tablist"
            aria-label="切换键盘配置层"
            className="inline-flex items-center gap-1 rounded-xl bg-[#EEF1F4] p-1 self-start"
          >
            {LAYERS.map((layer) => {
              const active = config.activeLayer === layer.id;
              return (
                <button
                  key={layer.id}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => onChange({ ...config, activeLayer: layer.id })}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-all duration-150 whitespace-nowrap ${
                    active
                      ? 'bg-white text-[#181C20] shadow-xs'
                      : 'text-[#586370] hover:text-[#181C20]'
                  }`}
                >
                  {layer.label}
                </button>
              );
            })}
          </div>
        </div>

        {/* Minimalist anodized aluminum tray + keycap grid */}
        <div className="mt-5 overflow-x-auto pb-1">
          <div className="min-w-[740px] rounded-2xl border border-[#DCE2E8] bg-[#EEF1F4] p-3.5 space-y-1.5">
            {KEYBOARD_75_ROWS.map((row, rowIdx) => (
              <div key={rowIdx} className="flex gap-1.5">
                {row.map((keyDef) => {
                  const isSelected = keyDef.code === selectedKeyCode;
                  const remap = currentLayerMap[keyDef.code];
                  const isSocd =
                    config.socdEnabled &&
                    config.socdPairs.some((pair) => pair.includes(keyDef.code));
                  const flexGrow = keyDef.width ?? 1;

                  return (
                    <button
                      key={keyDef.code}
                      type="button"
                      onClick={() => setSelectedKeyCode(keyDef.code)}
                      style={{ flex: `${flexGrow} 1 0%` }}
                      aria-label={`按键 ${keyDef.label}${remap ? `，已映射为 ${remap.label}` : ''}`}
                      className={`relative flex h-12 flex-col items-start justify-between rounded-lg border px-2 py-1.5 text-left transition-all ${
                        isSelected
                          ? 'border-[#2B6B68] bg-[#2B6B68] text-white shadow-xs'
                          : remap
                            ? 'border-[#A7CEC9] bg-[#EBF3F2] text-[#1D4E4C] hover:border-[#2B6B68]'
                            : 'border-[#D7DFE7] bg-white text-[#181C20] hover:border-[#9EB0C0]'
                      }`}
                    >
                      <span className="text-[11px] font-medium leading-none">{keyDef.label}</span>
                      <span
                        className={`w-full truncate text-[10px] leading-none ${
                          isSelected
                            ? 'text-white/85'
                            : remap
                              ? 'text-[#2B6B68] font-medium'
                              : isSocd
                                ? 'text-[#2B6B68]'
                                : 'text-[#84909D]'
                        }`}
                      >
                        {remap ? remap.label : isSocd ? 'SOCD' : ''}
                      </span>
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
        </div>

        {/* Selected Key Remap Inspector */}
        <div className="mt-5 grid grid-cols-1 gap-4 border-t border-[#EEF1F4] pt-5 lg:grid-cols-12">
          <div className="lg:col-span-4">
            <div className="text-xs text-[#84909D]">当前选中键位 ({config.activeLayer.toUpperCase()})</div>
            <div className="mt-1 flex items-baseline gap-2.5">
              <span className="font-mono text-lg font-semibold text-[#181C20]">
                {selectedKeyDef.label}
              </span>
              <span className="text-xs text-[#586370]">
                {selectedBinding ? `映射至：${selectedBinding.label}` : '保持原厂默认键值'}
              </span>
            </div>
            {selectedBinding && (
              <button
                type="button"
                onClick={() => handleAssignKey(null)}
                className="mt-2.5 rounded-lg border border-[#E5E9EE] px-3 py-1.5 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20]"
              >
                清除该键映射
              </button>
            )}
          </div>

          <div className="lg:col-span-8 space-y-3">
            <div>
              <div className="mb-1.5 text-[11px] text-[#84909D]">快速改键与多媒体</div>
              <div className="flex flex-wrap gap-1.5">
                {KEYBOARD_QUICK_REMAPS.map((item) => {
                  const active =
                    selectedBinding?.value === item.value && selectedBinding?.type === item.type;
                  return (
                    <button
                      key={`${item.type}-${item.value}`}
                      type="button"
                      onClick={() => handleAssignKey(item)}
                      className={`rounded-lg px-2.5 py-1.5 text-xs transition-colors whitespace-nowrap ${
                        active
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

            <div>
              <div className="mb-1.5 text-[11px] text-[#84909D]">绑定宏方案</div>
              <div className="flex flex-wrap gap-1.5">
                {macros.map((m) => {
                  const active =
                    selectedBinding?.type === 'macro' && selectedBinding.macroId === m.id;
                  return (
                    <button
                      key={m.id}
                      type="button"
                      onClick={() =>
                        handleAssignKey({
                          type: 'macro',
                          value: m.id,
                          label: `宏 · ${m.name}`,
                          macroId: m.id,
                        })
                      }
                      className={`rounded-lg px-2.5 py-1.5 text-xs transition-colors whitespace-nowrap ${
                        active
                          ? 'bg-[#2B6B68] text-white'
                          : 'bg-[#F0F3F6] text-[#181C20] hover:bg-[#EBF3F2] hover:text-[#1D4E4C]'
                      }`}
                    >
                      {m.name}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Bottom 2-Column: Hall-Effect Actuation + Competitive & Ambient Controls */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 lg:col-span-7">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-sm font-semibold text-[#181C20]">磁轴行程与 Rapid Trigger</h2>
              <p className="mt-0.5 text-xs text-[#586370]">
                精确控制霍尔传感器触发深度与动态抬起重置灵敏度。
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={config.rapidTriggerEnabled}
              onClick={() =>
                onChange({ ...config, rapidTriggerEnabled: !config.rapidTriggerEnabled })
              }
              className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
                config.rapidTriggerEnabled
                  ? 'bg-[#2B6B68] text-white'
                  : 'bg-[#F0F3F6] text-[#586370]'
              }`}
            >
              RT {config.rapidTriggerEnabled ? '已开启' : '已关闭'}
            </button>
          </div>

          <div className="mt-5 grid grid-cols-1 gap-6 sm:grid-cols-12 items-center">
            {/* Minimalist vertical travel visualizer */}
            <div className="sm:col-span-4 flex flex-col items-center rounded-xl bg-[#F6F7F9] p-4">
              <div className="text-[11px] text-[#84909D]">总键程 4.00 mm</div>
              <div className="relative mt-3 h-32 w-14 rounded-lg border border-[#D3DAE2] bg-white overflow-hidden">
                <div
                  className="absolute inset-x-0 top-0 bg-[#EBF3F2] border-b-2 border-[#2B6B68] transition-all duration-150"
                  style={{ height: `${actuationPercent}%` }}
                />
                <span className="absolute bottom-1.5 inset-x-0 text-center font-mono text-[10px] tabular-nums text-[#586370]">
                  {config.globalActuationMm.toFixed(2)}mm
                </span>
              </div>
              <div className="mt-2 text-[11px] text-[#586370]">触发深度示意</div>
            </div>

            <div className="sm:col-span-8 space-y-5">
              <div>
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="kbd-actuation" className="font-medium text-[#181C20]">
                    全局触发点 (Actuation Point)
                  </label>
                  <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                    {config.globalActuationMm.toFixed(2)} mm
                  </span>
                </div>
                <input
                  id="kbd-actuation"
                  type="range"
                  min={0.1}
                  max={4.0}
                  step={0.05}
                  value={config.globalActuationMm}
                  onChange={(e) =>
                    handleActuationChange(Number(e.target.value), config.rapidTriggerSensitivityMm)
                  }
                  className="lumen-slider mt-2 w-full"
                />
                <div className="mt-1 flex justify-between font-mono text-[11px] tabular-nums text-[#84909D]">
                  <span>0.10mm 极速</span>
                  <span>1.20mm 均衡</span>
                  <span>4.00mm 触底</span>
                </div>
              </div>

              <div className={config.rapidTriggerEnabled ? '' : 'opacity-50'}>
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="kbd-rt" className="font-medium text-[#181C20]">
                    Rapid Trigger 复位灵敏度
                  </label>
                  <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                    {config.rapidTriggerSensitivityMm.toFixed(2)} mm
                  </span>
                </div>
                <input
                  id="kbd-rt"
                  type="range"
                  min={0.05}
                  max={Math.min(2.0, config.globalActuationMm)}
                  step={0.05}
                  disabled={!config.rapidTriggerEnabled}
                  value={config.rapidTriggerSensitivityMm}
                  onChange={(e) =>
                    handleActuationChange(config.globalActuationMm, Number(e.target.value))
                  }
                  className="lumen-slider mt-2 w-full"
                />
              </div>
            </div>
          </div>
        </section>

        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 lg:col-span-5 space-y-5">
          <h2 className="text-sm font-semibold text-[#181C20]">竞技辅助与柔和背光</h2>

          <div className="space-y-2.5">
            {[
              {
                key: 'socdEnabled' as const,
                label: 'SOCD 优先响应 (A / D 急停)',
                desc: '同时按下反向移动键时优先响应后按按键',
              },
              {
                key: 'nkroEnabled' as const,
                label: '全键无冲 (NKRO)',
                desc: 'USB 与 2.4G 下支持任意多键同步触发',
              },
              {
                key: 'winKeyLocked' as const,
                label: '游戏模式禁用 Win 键',
                desc: '防止激烈操作时误触切出系统桌面',
              },
            ].map((item) => {
              const checked = config[item.key];
              return (
                <button
                  key={item.key}
                  type="button"
                  role="switch"
                  aria-checked={checked}
                  onClick={() => onChange({ ...config, [item.key]: !checked })}
                  className={`flex w-full items-center justify-between gap-3 rounded-xl border px-3.5 py-2.5 text-left transition-colors ${
                    checked
                      ? 'border-[#C4DCD9] bg-[#EBF3F2]/60'
                      : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#C9D3DC]'
                  }`}
                >
                  <div>
                    <div className="text-xs font-medium text-[#181C20]">{item.label}</div>
                    <div className="text-[11px] text-[#586370]">{item.desc}</div>
                  </div>
                  <span
                    className={`text-xs font-medium shrink-0 ${
                      checked ? 'text-[#1D4E4C]' : 'text-[#84909D]'
                    }`}
                  >
                    {checked ? '开启' : '关闭'}
                  </span>
                </button>
              );
            })}
          </div>

          <div className="border-t border-[#EEF1F4] pt-4 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-[#181C20]">极简氛围背光</span>
              <span className="font-mono tabular-nums text-[#586370]">
                亮度 {config.lightingMode === 'off' ? 0 : config.lightingBrightness}%
              </span>
            </div>
            <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
              {LIGHTING_MODES.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  onClick={() => onChange({ ...config, lightingMode: m.id })}
                  className={`rounded-lg py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
                    config.lightingMode === m.id
                      ? 'bg-[#181C20] text-white'
                      : 'bg-[#F4F6F8] text-[#586370] hover:bg-[#E5E9EE]'
                  }`}
                >
                  {m.label}
                </button>
              ))}
            </div>
            {config.lightingMode !== 'off' && (
              <input
                type="range"
                aria-label="背光亮度"
                min={5}
                max={100}
                step={5}
                value={config.lightingBrightness}
                onChange={(e) =>
                  onChange({ ...config, lightingBrightness: Number(e.target.value) })
                }
                className="lumen-slider w-full"
              />
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
