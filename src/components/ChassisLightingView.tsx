import {
  AlertTriangle,
  Cpu,
  History,
  Palette,
  Power,
  RotateCcw,
  ShieldCheck,
  Sliders,
  Sparkles,
} from 'lucide-react';
import { BUILTIN_CHASSIS_LIGHTING_PRESETS } from '../data/seedDevices';
import type { ChassisLightingConfig, ChassisLightingMode } from '../types/peripheral';
import type { ChassisRgbCapabilities } from '../services/deviceBridge';

interface ChassisLightingViewProps {
  capabilities: ChassisRgbCapabilities;
  config: ChassisLightingConfig;
  onChange: (next: ChassisLightingConfig) => void;
  onResetDefault: () => void;
  onRollback?: () => void;
}

const MODE_OPTIONS: { id: ChassisLightingMode; label: string; tag: string }[] = [
  { id: 'rainbow', label: '彩虹流光', tag: '硬件' },
  { id: 'wave', label: '光谱波浪', tag: '硬件' },
  { id: 'spectrum', label: '色彩循环', tag: '硬件' },
  { id: 'static', label: '纯色常亮', tag: '硬件' },
  { id: 'breathing', label: '静谧呼吸', tag: '硬件' },
  { id: 'chase-fade', label: '渐隐追逐', tag: '硬件' },
  { id: 'direct-gradient', label: '自定义双色渐变', tag: 'Direct' },
  { id: 'direct-per-header', label: '三针脚独立分色', tag: 'Direct' },
  { id: 'off', label: '全部熄灭', tag: '硬件' },
];

const QUICK_SWATCHES = [
  '#2B6B68',
  '#38BDF8',
  '#E2E8F0',
  '#10B981',
  '#818CF8',
  '#A855F7',
  '#F59E0B',
  '#EC4899',
];

function computeHeaderBarStyle(
  config: ChassisLightingConfig,
  zoneColor: string,
  enabled: boolean
): string {
  if (!enabled || config.mode === 'off') {
    return '#1E293B';
  }
  const opacity = Math.max(0.2, Math.min(1, config.brightness / 100));
  if (config.mode === 'rainbow' || config.mode === 'spectrum' || config.mode === 'wave') {
    return `linear-gradient(90deg, rgba(244,63,94,${opacity}), rgba(245,158,11,${opacity}), rgba(16,185,129,${opacity}), rgba(56,189,248,${opacity}), rgba(168,85,247,${opacity}))`;
  }
  if (config.mode === 'direct-gradient') {
    return `linear-gradient(90deg, ${config.primaryColor}, ${config.secondaryColor})`;
  }
  if (config.mode === 'direct-per-header' || config.perHeaderCustom) {
    return zoneColor;
  }
  return config.primaryColor;
}

export function ChassisLightingView({
  capabilities,
  config,
  onChange,
  onResetDefault,
  onRollback,
}: ChassisLightingViewProps) {
  const activePreset = BUILTIN_CHASSIS_LIGHTING_PRESETS.find((p) => p.id === config.presetId);
  const isDirectMode =
    config.mode === 'direct-gradient' || config.mode === 'direct-per-header';
  const showsSecondaryColor = config.mode === 'direct-gradient';

  const handleSelectPreset = (presetId: string) => {
    const preset = BUILTIN_CHASSIS_LIGHTING_PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    onChange({
      ...config,
      presetId: preset.id,
      mode: preset.mode,
      primaryColor: preset.primaryColor,
      secondaryColor: preset.secondaryColor,
      brightness: preset.brightness,
      speed: preset.speed,
      perHeaderCustom: preset.perHeaderCustom,
      headers: config.headers.map((h, idx) => ({
        ...h,
        enabled: preset.mode !== 'off',
        color: preset.headerColors?.[idx] ?? preset.primaryColor,
      })),
    });
  };

  const handleModeChange = (mode: ChassisLightingMode) => {
    onChange({
      ...config,
      presetId: 'custom',
      mode,
      perHeaderCustom: mode === 'direct-per-header' ? true : config.perHeaderCustom,
      headers:
        mode === 'off'
          ? config.headers.map((h) => ({ ...h, enabled: false }))
          : config.mode === 'off'
            ? config.headers.map((h) => ({ ...h, enabled: true }))
            : config.headers,
    });
  };

  const updateHeader = (
    idx: number,
    patch: Partial<ChassisLightingConfig['headers'][number]>
  ) => {
    onChange({
      ...config,
      presetId: 'custom',
      headers: config.headers.map((h, i) => (i === idx ? { ...h, ...patch } : h)),
    });
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">
            主机 ARGB 灯效中心
          </h1>
          <p className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[#586370]">
            <span className="inline-flex items-center gap-1 font-medium text-[#181C20]">
              <Cpu className="h-3.5 w-3.5 text-[#2B6B68]" />
              {capabilities.motherboard_name}
            </span>
            <span aria-hidden="true">·</span>
            <span>{capabilities.controller_name}</span>
            {capabilities.firmware_version && (
              <>
                <span aria-hidden="true">·</span>
                <span className="font-mono">FW {capabilities.firmware_version}</span>
              </>
            )}
            <span aria-hidden="true">·</span>
            <span>{capabilities.headers.length} 路可控灯区</span>
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {onRollback && (
            <button
              type="button"
              onClick={onRollback}
              className="inline-flex items-center gap-1.5 rounded-lg border border-[#D4DFDD] bg-white px-3.5 py-2 text-xs font-medium text-[#1D4E4C] hover:bg-[#EBF3F2] transition-colors whitespace-nowrap"
            >
              <History className="h-3.5 w-3.5" />
              <span>回滚上一版灯效</span>
            </button>
          )}

          <button
            type="button"
            onClick={onResetDefault}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[#2B6B68] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            <span>一键恢复出厂默认</span>
          </button>
        </div>
      </div>

      {/* Protocol & Safety Notice */}
      <div className="space-y-2">
        <div
          className={`flex items-start gap-2.5 rounded-xl border px-4 py-3 text-xs leading-relaxed ${
            capabilities.write_supported
              ? 'border-[#DCE5E4] bg-[#F4F8F7] text-[#1D4E4C]'
              : 'border-[#FDE68A] bg-[#FFFBEB] text-[#92400E]'
          }`}
        >
          <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <span className="font-medium">
              {capabilities.write_supported
                ? `${capabilities.vendor} (${capabilities.vendor_id}:${capabilities.product_id}) 安全直控已就绪：`
                : `${capabilities.vendor} 只读安全保护：`}{' '}
            </span>
            <span>{capabilities.note}</span>
          </div>
        </div>

        {capabilities.conflict_service_running && (
          <div className="flex items-start gap-2.5 rounded-xl border border-[#FDE68A] bg-[#FFFBEB] px-4 py-2.5 text-xs text-[#92400E]">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>
              检测到官方灯控服务 <span className="font-mono font-medium">{capabilities.conflict_service_running}</span>{' '}
              正在后台运行；若调整后灯效被瞬间覆盖，可在奥创中心退出接管或点击「一键恢复出厂默认」回归原生流光。
            </span>
          </div>
        )}
      </div>

      {/* Live ARGB Visual Strip Preview */}
      <section className="rounded-2xl border border-[#E5E9EE] bg-[#181C20] p-5 text-white">
        <div className="flex items-center justify-between text-xs">
          <div className="flex items-center gap-2">
            <Sparkles className="h-4 w-4 text-[#5EEAD4]" />
            <span className="font-medium">
              机箱实时灯带预览 · {activePreset ? activePreset.name : '自定义灯效'}
            </span>
          </div>
          <span className="font-mono text-[11px] text-[#94A3B8]">
            亮度 {config.mode === 'off' ? 0 : config.brightness}% · 速度 {config.speed}%
          </span>
        </div>

        <div className="mt-4 space-y-3">
          {capabilities.headers.map((hdr, idx) => {
            const zone = config.headers[idx] ?? {
              index: hdr.index,
              label: hdr.name,
              enabled: true,
              color: config.primaryColor,
              ledCount: hdr.default_leds,
            };
            const fill = computeHeaderBarStyle(config, zone.color, zone.enabled);
            return (
              <div key={`${hdr.index}-${hdr.name}`} className="space-y-1">
                <div className="flex items-center justify-between text-[11px] text-[#CBD5E1]">
                  <span>{zone.label}</span>
                  <span className="font-mono text-[#94A3B8]">
                    {zone.enabled && config.mode !== 'off'
                      ? `${zone.ledCount} LEDs · CH ${hdr.effect_channel}`
                      : '已熄灭'}
                  </span>
                </div>
                <div
                  className="h-2.5 w-full rounded-full transition-all duration-300"
                  style={{
                    background: fill,
                    opacity: zone.enabled && config.mode !== 'off' ? Math.max(0.25, config.brightness / 100) : 0.35,
                  }}
                />
              </div>
            );
          })}
        </div>
      </section>

      {/* Built-in Presets Grid */}
      <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 space-y-4">
        <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-sm font-semibold text-[#181C20]">内置主题灯效库（一键切换）</h2>
            <p className="mt-0.5 text-xs text-[#586370]">
              覆盖出厂流光、单色纯白、双色 Direct 逐珠渐变及三针脚水冷/风扇独立分色方案
            </p>
          </div>
          <span className="text-[11px] text-[#84909D]">
            共 {BUILTIN_CHASSIS_LIGHTING_PRESETS.length} 套安全预设
          </span>
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {BUILTIN_CHASSIS_LIGHTING_PRESETS.map((preset) => {
            const active = config.presetId === preset.id;
            const swatchBg =
              preset.mode === 'rainbow' || preset.mode === 'wave'
                ? 'linear-gradient(135deg, #F43F5E, #10B981, #38BDF8, #A855F7)'
                : preset.mode === 'direct-gradient'
                  ? `linear-gradient(135deg, ${preset.primaryColor}, ${preset.secondaryColor})`
                  : preset.primaryColor;
            return (
              <button
                key={preset.id}
                type="button"
                onClick={() => handleSelectPreset(preset.id)}
                className={`flex flex-col justify-between rounded-xl border p-3.5 text-left transition-all ${
                  active
                    ? 'border-[#2B6B68] bg-[#EBF3F2]/60 ring-1 ring-[#2B6B68]'
                    : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#CBD3DC] hover:bg-white'
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-semibold text-[#181C20]">{preset.name}</span>
                  <span
                    className="h-4 w-4 shrink-0 rounded-full border border-black/10"
                    style={{ background: swatchBg }}
                  />
                </div>
                <p className="mt-2 text-[11px] leading-relaxed text-[#586370]">
                  {preset.description}
                </p>
              </button>
            );
          })}
        </div>
      </section>

      {/* Custom Modes, Color & Brightness + Per-Header Controls */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        {/* Left: Mode & Global Color */}
        <section className="lg:col-span-5 rounded-2xl border border-[#E5E9EE] bg-white p-6 space-y-5">
          <div className="flex items-center gap-2">
            <Palette className="h-4 w-4 text-[#2B6B68]" />
            <h2 className="text-sm font-semibold text-[#181C20]">自定义模式与色彩调校</h2>
          </div>

          <div className="flex flex-wrap gap-1.5">
            {MODE_OPTIONS.filter((m) =>
              capabilities.supported_modes.includes(m.id) ||
              m.id === 'static' ||
              m.id === 'rainbow' ||
              m.id === 'off'
            ).map((opt) => {
              const active = config.mode === opt.id;
              return (
                <button
                  key={opt.id}
                  type="button"
                  onClick={() => handleModeChange(opt.id)}
                  className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors ${
                    active
                      ? 'bg-[#2B6B68] text-white'
                      : 'bg-[#F0F3F6] text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20]'
                  }`}
                >
                  <span>{opt.label}</span>
                  <span
                    className={`text-[10px] ${
                      active ? 'text-[#CCFBF1]' : 'text-[#84909D]'
                    }`}
                  >
                    {opt.tag}
                  </span>
                </button>
              );
            })}
          </div>

          {/* Primary / Secondary Color */}
          <div className="space-y-3 border-t border-[#F0F3F6] pt-4">
            <div className="flex items-center justify-between">
              <label htmlFor="chassis-primary-color" className="text-xs font-medium text-[#181C20]">
                {showsSecondaryColor ? '渐变起始色' : '主灯效色彩'}
              </label>
              <div className="flex items-center gap-2">
                <span className="font-mono text-xs text-[#586370]">
                  {config.primaryColor.toUpperCase()}
                </span>
                <input
                  id="chassis-primary-color"
                  type="color"
                  value={config.primaryColor}
                  onChange={(e) =>
                    onChange({
                      ...config,
                      presetId: 'custom',
                      primaryColor: e.target.value,
                    })
                  }
                  className="h-7 w-9 cursor-pointer rounded border border-[#E5E9EE] bg-white p-0.5"
                />
              </div>
            </div>

            {showsSecondaryColor && (
              <div className="flex items-center justify-between">
                <label
                  htmlFor="chassis-secondary-color"
                  className="text-xs font-medium text-[#181C20]"
                >
                  渐变终点色
                </label>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-xs text-[#586370]">
                    {config.secondaryColor.toUpperCase()}
                  </span>
                  <input
                    id="chassis-secondary-color"
                    type="color"
                    value={config.secondaryColor}
                    onChange={(e) =>
                      onChange({
                        ...config,
                        presetId: 'custom',
                        secondaryColor: e.target.value,
                      })
                    }
                    className="h-7 w-9 cursor-pointer rounded border border-[#E5E9EE] bg-white p-0.5"
                  />
                </div>
              </div>
            )}

            <div className="flex flex-wrap gap-2 pt-1">
              {QUICK_SWATCHES.map((hex) => (
                <button
                  key={hex}
                  type="button"
                  aria-label={`选择预设颜色 ${hex}`}
                  onClick={() =>
                    onChange({
                      ...config,
                      presetId: 'custom',
                      primaryColor: hex,
                    })
                  }
                  className={`h-6 w-6 rounded-full border transition-transform hover:scale-110 ${
                    config.primaryColor.toUpperCase() === hex
                      ? 'border-[#181C20] ring-2 ring-[#2B6B68]/40'
                      : 'border-black/15'
                  }`}
                  style={{ backgroundColor: hex }}
                />
              ))}
            </div>
          </div>

          {/* Brightness & Speed */}
          <div className="space-y-4 border-t border-[#F0F3F6] pt-4">
            <div>
              <div className="mb-1.5 flex items-center justify-between text-xs">
                <span className="font-medium text-[#181C20]">灯效亮度（Direct 实时缩放）</span>
                <span className="font-mono tabular-nums text-[#2B6B68]">{config.brightness}%</span>
              </div>
              <input
                type="range"
                min={10}
                max={100}
                step={5}
                value={config.brightness}
                onChange={(e) =>
                  onChange({
                    ...config,
                    presetId: 'custom',
                    brightness: Number(e.target.value),
                  })
                }
                className="w-full accent-[#2B6B68]"
              />
            </div>

            {!isDirectMode && (
              <div>
                <div className="mb-1.5 flex items-center justify-between text-xs">
                  <span className="font-medium text-[#181C20]">流光速度</span>
                  <span className="font-mono tabular-nums text-[#586370]">{config.speed}%</span>
                </div>
                <input
                  type="range"
                  min={10}
                  max={100}
                  step={5}
                  value={config.speed}
                  onChange={(e) =>
                    onChange({
                      ...config,
                      presetId: 'custom',
                      speed: Number(e.target.value),
                    })
                  }
                  className="w-full accent-[#2B6B68]"
                />
              </div>
            )}
          </div>
        </section>

        {/* Right: Per-Header ARGB Matrix */}
        <section className="lg:col-span-7 rounded-2xl border border-[#E5E9EE] bg-white p-6 space-y-4">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-2">
              <Sliders className="h-4 w-4 text-[#2B6B68]" />
              <div>
                <h2 className="text-sm font-semibold text-[#181C20]">
                  主板 ARGB 针脚独立通道配置
                </h2>
                <p className="text-[11px] text-[#586370]">
                  可为水冷冷头、正/反叶风扇所属的每路 5V 3-Pin 针脚自定义备注、灯珠数与独立颜色
                </p>
              </div>
            </div>

            <label className="inline-flex items-center gap-2 text-xs font-medium text-[#1D4E4C] cursor-pointer select-none">
              <input
                type="checkbox"
                checked={config.perHeaderCustom}
                onChange={(e) =>
                  onChange({
                    ...config,
                    presetId: 'custom',
                    perHeaderCustom: e.target.checked,
                  })
                }
                className="rounded border-[#CBD3DC] accent-[#2B6B68]"
              />
              <span>分针脚独立配色</span>
            </label>
          </div>

          <div className="space-y-3">
            {capabilities.headers.map((hdr, idx) => {
              const zone = config.headers[idx] ?? {
                index: hdr.index,
                label: hdr.name,
                enabled: true,
                color: config.primaryColor,
                ledCount: hdr.default_leds,
              };
              return (
                <div
                  key={`${hdr.index}-${hdr.name}`}
                  className="rounded-xl border border-[#E5E9EE] bg-[#FAFBFD] p-4 space-y-3"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      <span className="rounded bg-[#EBF3F2] px-2 py-0.5 font-mono text-[11px] font-medium text-[#1D4E4C] shrink-0">
                        {hdr.name}
                      </span>
                      <input
                        type="text"
                        value={zone.label}
                        onChange={(e) => updateHeader(idx, { label: e.target.value })}
                        placeholder="输入备注（如：冰封无限360冷头）"
                        className="min-w-0 flex-1 rounded-md border border-[#E5E9EE] bg-white px-2.5 py-1 text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                      />
                    </div>

                    <div className="flex items-center gap-2 shrink-0">
                      <input
                        type="color"
                        value={zone.color}
                        onChange={(e) => updateHeader(idx, { color: e.target.value })}
                        aria-label={`${hdr.name} 颜色`}
                        className="h-7 w-8 cursor-pointer rounded border border-[#E5E9EE] bg-white p-0.5"
                      />
                      <button
                        type="button"
                        onClick={() => updateHeader(idx, { enabled: !zone.enabled })}
                        className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-[11px] font-medium transition-colors ${
                          zone.enabled
                            ? 'bg-[#EBF3F2] text-[#1D4E4C]'
                            : 'bg-[#E5E9EE] text-[#586370]'
                        }`}
                      >
                        <Power className="h-3 w-3" />
                        <span>{zone.enabled ? '已开启' : '已关闭'}</span>
                      </button>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    <span className="text-[11px] text-[#586370] shrink-0">Gen2 灯珠规模</span>
                    <input
                      type="range"
                      min={8}
                      max={Math.max(16, hdr.max_leds)}
                      step={2}
                      value={zone.ledCount}
                      onChange={(e) =>
                        updateHeader(idx, { ledCount: Number(e.target.value) })
                      }
                      className="flex-1 accent-[#2B6B68]"
                    />
                    <span className="w-16 text-right font-mono text-[11px] tabular-nums text-[#181C20]">
                      {zone.ledCount} / {hdr.max_leds}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>

          <div className="rounded-xl border border-[#EEF1F4] bg-[#F8FAFB] px-3.5 py-2.5 text-[11px] leading-relaxed text-[#586370]">
            已适配主流主板协议族：华硕 ASUS Aura Sync USB (0x0B05)、技嘉 RGB Fusion 2.0 (IT8297/IT57xx)、华擎 Polychrome USB (0x26CE)；微星 Mystic Light (0x1462) 默认锁定只读保护防变砖。
          </div>
        </section>
      </div>
    </div>
  );
}
