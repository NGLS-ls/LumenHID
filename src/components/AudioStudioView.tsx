import { useEffect, useRef, useState } from 'react';
import { Download, History, Plus, RotateCcw, Sparkles, Volume2, VolumeX } from 'lucide-react';
import { BUILTIN_EQ_PRESETS, EQ_LABELS } from '../data/seedDevices';
import type {
  AudioConfig,
  AudioPreset,
  PeripheralDevice,
  RoomAcousticsMode,
} from '../types/peripheral';
import {
  applyEqPreset,
  calculateSpatialNodePosition,
  clampEqGain,
  generateEqCurvePath,
  generateEqualizerApoConfig,
} from '../utils/deviceEngine';
import {
  AudioPreviewEngine,
  downloadTextFile,
  type ThxStatus,
} from '../services/deviceBridge';

interface AudioStudioViewProps {
  device: PeripheralDevice | null;
  config: AudioConfig;
  thxStatus?: ThxStatus | null;
  onChange: (next: AudioConfig) => void;
  onReset: () => void;
  onRollback?: () => void;
  onChangeLedMode?: (mode: number) => void;
  onExportApoNotice?: (message: string) => void;
}

const ROOM_MODES: { id: RoomAcousticsMode; label: string }[] = [
  { id: 'dry', label: '纯干声' },
  { id: 'studio', label: '监听棚' },
  { id: 'hall', label: '音乐厅' },
];

const LED_MODES: { mode: number; label: string }[] = [
  { mode: 0, label: '关闭' },
  { mode: 1, label: '常亮' },
  { mode: 2, label: '呼吸' },
  { mode: 3, label: '幻彩循环' },
];

export function AudioStudioView({
  device,
  config,
  thxStatus,
  onChange,
  onReset,
  onRollback,
  onChangeLedMode,
  onExportApoNotice,
}: AudioStudioViewProps) {
  const [auditioning, setAuditioning] = useState(false);
  const [activeLedMode, setActiveLedMode] = useState<number>(3);
  const [customPresetName, setCustomPresetName] = useState('');
  const [presetError, setPresetError] = useState<string | null>(null);
  const [selectedChannelId, setSelectedChannelId] = useState<AudioConfig['spatialChannels'][number]['id']>('C');
  const audioEngineRef = useRef<AudioPreviewEngine | null>(null);

  const allPresets: AudioPreset[] = [...BUILTIN_EQ_PRESETS, ...config.customPresets];
  const activePreset = allPresets.find((p) => p.id === config.activePresetId);
  // 1000px wide with 50px side padding places 10 nodes at 50, 150, ..., 950 (exact centers of 10 equal columns)
  const curve = generateEqCurvePath(config.bands, 1000, 180, 50, 20);
  const selectedChannel =
    config.spatialChannels.find((ch) => ch.id === selectedChannelId) ?? config.spatialChannels[0];

  const handleChannelGainChange = (gainDb: number) => {
    onChange({
      ...config,
      spatialChannels: config.spatialChannels.map((ch) =>
        ch.id === selectedChannel.id ? { ...ch, gainDb: clampEqGain(gainDb) } : ch
      ),
    });
  };

  const handleExportEqualizerApo = () => {
    const content = generateEqualizerApoConfig(
      config.bands,
      config.preampDb,
      device?.name ?? 'Windows Audio Device'
    );
    downloadTextFile('LumenHID-EQ.txt', content, 'text/plain;charset=utf-8');
    onExportApoNotice?.('已导出 EqualizerAPO 全局 10 段 EQ 配置文件 LumenHID-EQ.txt');
  };

  useEffect(() => {
    audioEngineRef.current = new AudioPreviewEngine();
    return () => {
      audioEngineRef.current?.stop();
    };
  }, []);

  useEffect(() => {
    if (auditioning) {
      audioEngineRef.current?.update(config.bands, config.preampDb, config.gameChatBalance);
    }
  }, [auditioning, config.bands, config.preampDb, config.gameChatBalance]);

  const toggleAudition = () => {
    const engine = audioEngineRef.current;
    if (!engine) return;
    if (auditioning) {
      engine.stop();
      setAuditioning(false);
    } else {
      const ok = engine.start(config.bands, config.preampDb, config.gameChatBalance);
      setAuditioning(ok);
    }
  };

  const handleSelectPreset = (presetId: string) => {
    const applied = applyEqPreset(allPresets, presetId);
    if (!applied) return;
    onChange({
      ...config,
      activePresetId: presetId,
      bands: applied.bands,
      preampDb: applied.preampDb,
    });
  };

  const handleBandChange = (idx: number, value: number) => {
    const nextBands = config.bands.map((b, i) => (i === idx ? clampEqGain(value) : b));
    onChange({
      ...config,
      activePresetId: 'custom',
      bands: nextBands,
    });
  };

  const handleSaveCustomPreset = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = customPresetName.trim();
    if (!trimmed) {
      setPresetError('请输入自定义预设名称');
      return;
    }
    const created: AudioPreset = {
      id: `custom-${Date.now()}`,
      name: trimmed,
      description: '用户自定义 10 段均衡曲线',
      bands: [...config.bands],
      preampDb: config.preampDb,
      builtIn: false,
    };
    onChange({
      ...config,
      activePresetId: created.id,
      customPresets: [...config.customPresets, created],
    });
    setCustomPresetName('');
    setPresetError(null);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">
            耳机均衡器与空间声场
          </h1>
          <p className="mt-1 flex flex-wrap items-center gap-2 text-xs text-[#586370]">
            <span>{device ? device.name : '未连接外置耳机（支持实时试听与 EqualizerAPO 导出）'}</span>
            {device?.batteryPercent !== null && device?.batteryPercent !== undefined && (
              <>
                <span aria-hidden="true">·</span>
                <span className="font-mono font-medium tabular-nums text-[#2B6B68]">
                  电量 {device.batteryPercent}%
                </span>
              </>
            )}
            <span aria-hidden="true">·</span>
            <span>{activePreset ? activePreset.name : '自定义曲线'}</span>
            <span aria-hidden="true">·</span>
            <span>{config.surround71Enabled ? `7.1 虚拟环绕 · 声场宽度 ${config.soundstageWidth}%` : '双声道立体声'}</span>
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {thxStatus && (
            <span
              className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-2 text-[11px] font-medium whitespace-nowrap ${
                thxStatus.available
                  ? 'border-[#C4DCD9] bg-[#EBF3F2] text-[#1D4E4C]'
                  : 'border-[#E5E9EE] bg-[#F4F6F8] text-[#586370]'
              }`}
              title={thxStatus.detail}
            >
              <span
                className={`h-1.5 w-1.5 rounded-full ${
                  thxStatus.available ? 'bg-[#1D7A5F]' : 'bg-[#94A3B8]'
                }`}
              />
              <span>
                {thxStatus.available
                  ? `THX gRPC v${thxStatus.version ?? 'Online'}`
                  : 'THX 服务离线 · APO 兜底'}
              </span>
            </span>
          )}

          <button
            type="button"
            onClick={handleExportEqualizerApo}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[#C4DCD9] bg-[#EBF3F2] px-3.5 py-2 text-xs font-medium text-[#1D4E4C] hover:bg-[#DCEBE9] transition-colors whitespace-nowrap"
          >
            <Download className="h-3.5 w-3.5" />
            <span>导出 EqualizerAPO (.txt)</span>
          </button>

          <button
            type="button"
            onClick={toggleAudition}
            className={`inline-flex items-center gap-1.5 rounded-lg px-3.5 py-2 text-xs font-medium transition-colors whitespace-nowrap ${
              auditioning
                ? 'bg-[#2B6B68] text-white hover:bg-[#215452]'
                : 'border border-[#E5E9EE] bg-white text-[#181C20] hover:bg-[#F0F3F6]'
            }`}
          >
            {auditioning ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
            <span>{auditioning ? '停止声学试听' : '实时 EQ 试听'}</span>
          </button>

          {onRollback && (
            <button
              type="button"
              onClick={onRollback}
              className="inline-flex items-center gap-1.5 rounded-lg border border-[#D4DFDD] bg-white px-3 py-2 text-xs font-medium text-[#1D4E4C] hover:bg-[#EBF3F2] transition-colors whitespace-nowrap"
            >
              <History className="h-3.5 w-3.5" />
              <span>回滚上一版音效</span>
            </button>
          )}

          <button
            type="button"
            onClick={onReset}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] transition-colors whitespace-nowrap"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            <span>重置平直</span>
          </button>
        </div>
      </div>

      {device?.protocolNote && (
        <div className="rounded-xl border border-[#DCE5E4] bg-[#F4F8F7] px-4 py-2.5 text-xs leading-relaxed text-[#1D4E4C]">
          {device.protocolNote}
        </div>
      )}

      {device?.ledPath && onChangeLedMode && (
        <div className="flex flex-col gap-2.5 rounded-2xl border border-[#E5E9EE] bg-white px-5 py-3.5 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-center gap-2 text-xs font-medium text-[#181C20]">
            <Sparkles className="h-4 w-4 text-[#2B6B68]" />
            <span>HECATE G5 Max 耳机 RGB 氛围灯效（HID 0xED 安全直写）</span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {LED_MODES.map((item) => {
              const active = activeLedMode === item.mode;
              return (
                <button
                  key={item.mode}
                  type="button"
                  onClick={() => {
                    setActiveLedMode(item.mode);
                    onChangeLedMode(item.mode);
                  }}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
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
      )}

      {/* 10-Band EQ Section */}
      <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 space-y-5">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <div>
            <h2 className="text-sm font-semibold text-[#181C20]">10 段高精度图示均衡器</h2>
            <p className="mt-0.5 text-xs text-[#586370]">
              {activePreset?.description ?? '自由调节 31Hz–16kHz 各频段增益，支持 Web Audio 实时滤波试听。'}
            </p>
          </div>

          <div className="flex flex-wrap gap-1.5">
            {allPresets.map((preset) => {
              const active = config.activePresetId === preset.id;
              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => handleSelectPreset(preset.id)}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
                    active
                      ? 'bg-[#2B6B68] text-white'
                      : 'bg-[#F0F3F6] text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20]'
                  }`}
                >
                  {preset.name}
                </button>
              );
            })}
          </div>
        </div>

        {/* Smooth SVG Frequency Response Curve aligned 1:1 with the 10 columns below */}
        <div className="rounded-xl border border-[#EEF1F4] bg-[#FAFBFD] px-1 py-3">
          <svg
            viewBox="0 0 1000 180"
            className="w-full h-auto max-h-48 select-none"
            role="img"
            aria-label="10 段均衡器频响曲线图"
          >
            <defs>
              <linearGradient id="eqCurveFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="#2B6B68" stopOpacity="0.18" />
                <stop offset="100%" stopColor="#2B6B68" stopOpacity="0.01" />
              </linearGradient>
            </defs>

            {/* Horizontal dB Gridlines (+12, +6, 0, -6, -12) */}
            {[
              { y: 20, label: '+12' },
              { y: 55, label: '+6' },
              { y: 90, label: '0 dB' },
              { y: 125, label: '-6' },
              { y: 160, label: '-12' },
            ].map((row, idx) => (
              <g key={row.y}>
                <line
                  x1="36"
                  y1={row.y}
                  x2="972"
                  y2={row.y}
                  stroke={idx === 2 ? '#C4CFD9' : '#E8EDF2'}
                  strokeWidth={idx === 2 ? '1.3' : '1'}
                  strokeDasharray={idx === 2 ? undefined : '4 4'}
                />
                <text
                  x="30"
                  y={row.y + 3.5}
                  textAnchor="end"
                  fontSize="10"
                  fontFamily="JetBrains Mono, monospace"
                  fill="#84909D"
                >
                  {row.label}
                </text>
              </g>
            ))}

            {/* Vertical frequency guide lines matching 10 column centers */}
            {curve.points.map((pt, idx) => (
              <line
                key={idx}
                x1={pt.x}
                y1="20"
                x2={pt.x}
                y2="160"
                stroke="#EEF2F6"
                strokeWidth="1"
              />
            ))}

            <path d={curve.areaPath} fill="url(#eqCurveFill)" />
            <path
              d={curve.linePath}
              fill="none"
              stroke="#2B6B68"
              strokeWidth="2.4"
              strokeLinecap="round"
            />

            {curve.points.map((pt, idx) => (
              <g key={idx}>
                <circle
                  cx={pt.x}
                  cy={pt.y}
                  r="5"
                  fill="#FFFFFF"
                  stroke="#2B6B68"
                  strokeWidth="2.2"
                />
              </g>
            ))}
          </svg>
        </div>

        {/* 10-Band Individual Precision Faders */}
        <div className="grid grid-cols-5 gap-3 sm:grid-cols-10">
          {config.bands.map((db, idx) => (
            <div key={EQ_LABELS[idx]} className="flex flex-col items-center">
              <span className="font-mono text-[11px] font-medium tabular-nums text-[#2B6B68]">
                {db > 0 ? `+${db.toFixed(1)}` : db.toFixed(1)}
              </span>
              <div className="my-2 flex h-28 items-center justify-center">
                <input
                  type="range"
                  aria-label={`${EQ_LABELS[idx]} 频段增益`}
                  min={-12}
                  max={12}
                  step={0.5}
                  value={db}
                  onChange={(e) => handleBandChange(idx, Number(e.target.value))}
                  className="lumen-slider w-24 -rotate-90"
                />
              </div>
              <span className="font-mono text-[11px] text-[#586370]">{EQ_LABELS[idx]}</span>
            </div>
          ))}
        </div>

        {/* Preamp + Save Custom Preset */}
        <div className="flex flex-col gap-4 border-t border-[#EEF1F4] pt-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex items-center gap-4 flex-1 max-w-sm">
            <label htmlFor="eq-preamp" className="text-xs font-medium text-[#181C20] whitespace-nowrap">
              前置增益 (Pre-amp)
            </label>
            <input
              id="eq-preamp"
              type="range"
              min={-6}
              max={6}
              step={0.5}
              value={config.preampDb}
              onChange={(e) =>
                onChange({ ...config, preampDb: Number(e.target.value), activePresetId: 'custom' })
              }
              className="lumen-slider flex-1"
            />
            <span className="w-14 text-right font-mono text-xs font-medium tabular-nums text-[#181C20]">
              {config.preampDb > 0 ? `+${config.preampDb.toFixed(1)}` : config.preampDb.toFixed(1)} dB
            </span>
          </div>

          <form onSubmit={handleSaveCustomPreset} className="flex flex-wrap items-center gap-2">
            <input
              type="text"
              value={customPresetName}
              onChange={(e) => {
                setCustomPresetName(e.target.value);
                setPresetError(null);
              }}
              placeholder="保存为自定义预设..."
              aria-label="自定义 EQ 预设名称"
              className="w-44 rounded-lg border border-[#E5E9EE] px-3 py-1.5 text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
            />
            <button
              type="submit"
              className="inline-flex items-center gap-1 rounded-lg bg-[#181C20] px-3 py-1.5 text-xs font-medium text-white hover:bg-[#2C333B] whitespace-nowrap"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>保存预设</span>
            </button>
            {presetError && <span className="text-xs text-[#B91C1C]">{presetError}</span>}
          </form>
        </div>
      </section>

      {/* Bottom 2-Column: 7.1 Spatial Soundstage + Mic & Game/Chat Mixer */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 lg:col-span-7">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-sm font-semibold text-[#181C20]">7.1 虚拟声场与方位定位</h2>
              <p className="mt-0.5 text-xs text-[#586370]">
                调整虚拟扬声器方位距离与空间宽度，增强三维听声辨位层次。
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={config.surround71Enabled}
              onClick={() =>
                onChange({ ...config, surround71Enabled: !config.surround71Enabled })
              }
              className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
                config.surround71Enabled
                  ? 'bg-[#2B6B68] text-white'
                  : 'bg-[#F0F3F6] text-[#586370]'
              }`}
            >
              7.1 环绕 {config.surround71Enabled ? '已开启' : '已关闭'}
            </button>
          </div>

          <div className="mt-5 grid grid-cols-1 gap-6 sm:grid-cols-12 items-center">
            {/* Polar Soundstage Radar */}
            <div className="sm:col-span-5 flex flex-col items-center">
              <svg
                viewBox="0 0 240 240"
                className="w-full max-w-[215px] select-none"
                role="img"
                aria-label="7.1 虚拟声场极坐标方位图"
              >
                <circle cx="120" cy="120" r="96" fill="#FAFBFD" stroke="#E5E9EE" strokeWidth="1.2" />
                <circle cx="120" cy="120" r="64" fill="none" stroke="#E8EDF2" strokeDasharray="3 3" />
                <circle cx="120" cy="120" r="32" fill="none" stroke="#E8EDF2" strokeDasharray="3 3" />
                <line x1="120" y1="24" x2="120" y2="216" stroke="#EEF1F4" />
                <line x1="24" y1="120" x2="216" y2="120" stroke="#EEF1F4" />

                {/* Listener center mark */}
                <circle cx="120" cy="120" r="10" fill="#181C20" />
                <path d="M120 105 L125 113 L115 113 Z" fill="#2B6B68" />

                {/* 7 virtual speaker nodes */}
                {config.spatialChannels.map((ch) => {
                  const pos = calculateSpatialNodePosition(
                    ch,
                    config.surround71Enabled ? config.soundstageWidth : 40,
                    120,
                    120,
                    88
                  );
                  const isSelected = ch.id === selectedChannel.id;
                  return (
                    <g
                      key={ch.id}
                      onClick={() => setSelectedChannelId(ch.id)}
                      opacity={config.surround71Enabled ? 1 : 0.35}
                      className="cursor-pointer"
                    >
                      <line
                        x1="120"
                        y1="120"
                        x2={pos.x}
                        y2={pos.y}
                        stroke={isSelected ? '#2B6B68' : '#C4DCD9'}
                        strokeWidth={isSelected ? '1.6' : '1'}
                      />
                      <circle
                        cx={pos.x}
                        cy={pos.y}
                        r={isSelected ? '13.5' : '12'}
                        fill={isSelected ? '#2B6B68' : '#FFFFFF'}
                        stroke="#2B6B68"
                        strokeWidth="1.8"
                      />
                      <text
                        x={pos.x}
                        y={pos.y + 3.5}
                        textAnchor="middle"
                        fontSize="9"
                        fontFamily="JetBrains Mono, monospace"
                        fontWeight="600"
                        fill={isSelected ? '#FFFFFF' : '#181C20'}
                      >
                        {ch.id}
                      </text>
                    </g>
                  );
                })}
              </svg>
            </div>

            {/* Soundstage Sliders, Room Acoustics & Individual 7.1 Channel Gain */}
            <div className="sm:col-span-7 space-y-4">
              <div>
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="soundstage-width" className="font-medium text-[#181C20]">
                    声场开阔度 (Soundstage Width)
                  </label>
                  <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                    {config.soundstageWidth}%
                  </span>
                </div>
                <input
                  id="soundstage-width"
                  type="range"
                  min={0}
                  max={100}
                  value={config.soundstageWidth}
                  onChange={(e) =>
                    onChange({ ...config, soundstageWidth: Number(e.target.value) })
                  }
                  className="lumen-slider mt-2 w-full"
                />
              </div>

              <div className="rounded-xl bg-[#F7F8FA] p-3 space-y-2.5">
                <div className="flex items-center justify-between text-xs">
                  <label htmlFor="spatial-channel-gain" className="font-medium text-[#181C20]">
                    声道独立微调 · {selectedChannel.label}
                  </label>
                  <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                    {selectedChannel.gainDb > 0
                      ? `+${selectedChannel.gainDb.toFixed(1)}`
                      : selectedChannel.gainDb.toFixed(1)}{' '}
                    dB
                  </span>
                </div>
                <div className="flex flex-wrap gap-1">
                  {config.spatialChannels.map((ch) => (
                    <button
                      key={ch.id}
                      type="button"
                      onClick={() => setSelectedChannelId(ch.id)}
                      className={`rounded-md px-2 py-1 font-mono text-[11px] font-medium transition-colors ${
                        ch.id === selectedChannel.id
                          ? 'bg-[#2B6B68] text-white'
                          : 'bg-white text-[#586370] hover:text-[#181C20]'
                      }`}
                    >
                      {ch.id}
                    </button>
                  ))}
                </div>
                <input
                  id="spatial-channel-gain"
                  type="range"
                  min={-12}
                  max={12}
                  step={0.5}
                  disabled={!config.surround71Enabled}
                  value={selectedChannel.gainDb}
                  onChange={(e) => handleChannelGainChange(Number(e.target.value))}
                  className="lumen-slider w-full disabled:opacity-40"
                />
              </div>

              <div>
                <div className="text-xs font-medium text-[#181C20]">房间声学环境</div>
                <div className="mt-2 grid grid-cols-3 gap-2">
                  {ROOM_MODES.map((mode) => (
                    <button
                      key={mode.id}
                      type="button"
                      onClick={() => onChange({ ...config, roomAcoustics: mode.id })}
                      className={`rounded-lg py-1.5 text-xs font-medium transition-colors ${
                        config.roomAcoustics === mode.id
                          ? 'bg-[#181C20] text-white'
                          : 'bg-[#F4F6F8] text-[#586370] hover:bg-[#E5E9EE]'
                      }`}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3 pt-1">
                <div>
                  <div className="flex justify-between text-xs">
                    <label htmlFor="bass-boost" className="text-[#586370]">低频增强</label>
                    <span className="font-mono tabular-nums text-[#181C20]">{config.bassBoost}%</span>
                  </div>
                  <input
                    id="bass-boost"
                    type="range"
                    min={0}
                    max={100}
                    value={config.bassBoost}
                    onChange={(e) => onChange({ ...config, bassBoost: Number(e.target.value) })}
                    className="lumen-slider mt-1.5 w-full"
                  />
                </div>
                <div>
                  <div className="flex justify-between text-xs">
                    <label htmlFor="voice-clarity" className="text-[#586370]">人声清晰度</label>
                    <span className="font-mono tabular-nums text-[#181C20]">{config.voiceClarity}%</span>
                  </div>
                  <input
                    id="voice-clarity"
                    type="range"
                    min={0}
                    max={100}
                    value={config.voiceClarity}
                    onChange={(e) => onChange({ ...config, voiceClarity: Number(e.target.value) })}
                    className="lumen-slider mt-1.5 w-full"
                  />
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Mic & Game/Chat Mixer */}
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 lg:col-span-5 space-y-5">
          <h2 className="text-sm font-semibold text-[#181C20]">麦克风与双通道混音</h2>

          <div className="space-y-4">
            <div>
              <div className="flex justify-between text-xs">
                <label htmlFor="mic-gain" className="font-medium text-[#181C20]">麦克风输入增益</label>
                <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                  {config.micGainPercent}%
                </span>
              </div>
              <input
                id="mic-gain"
                type="range"
                min={0}
                max={100}
                value={config.micGainPercent}
                onChange={(e) => onChange({ ...config, micGainPercent: Number(e.target.value) })}
                className="lumen-slider mt-2 w-full"
              />
            </div>

            <div>
              <div className="flex justify-between text-xs">
                <label htmlFor="mic-sidetone" className="font-medium text-[#181C20]">实时耳返监听 (Sidetone)</label>
                <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                  {config.sidetonePercent}%
                </span>
              </div>
              <input
                id="mic-sidetone"
                type="range"
                min={0}
                max={100}
                value={config.sidetonePercent}
                onChange={(e) => onChange({ ...config, sidetonePercent: Number(e.target.value) })}
                className="lumen-slider mt-2 w-full"
              />
            </div>

            <div>
              <div className="flex justify-between text-xs">
                <label htmlFor="mic-gate" className="font-medium text-[#181C20]">环境噪声门阈值</label>
                <span className="font-mono font-semibold tabular-nums text-[#2B6B68]">
                  {config.noiseGateDb} dB
                </span>
              </div>
              <input
                id="mic-gate"
                type="range"
                min={-60}
                max={-10}
                value={config.noiseGateDb}
                onChange={(e) => onChange({ ...config, noiseGateDb: Number(e.target.value) })}
                className="lumen-slider mt-2 w-full"
              />
            </div>

            <div className="border-t border-[#EEF1F4] pt-4">
              <div className="flex justify-between text-xs">
                <label htmlFor="game-chat-bal" className="font-medium text-[#181C20]">
                  游戏 / 语音混音平衡
                </label>
                <span className="font-mono tabular-nums text-[#586370]">
                  {config.gameChatBalance === 0
                    ? '均衡 50/50'
                    : config.gameChatBalance < 0
                      ? `偏向游戏 ${Math.abs(config.gameChatBalance)}%`
                      : `偏向语音 ${config.gameChatBalance}%`}
                </span>
              </div>
              <input
                id="game-chat-bal"
                type="range"
                min={-100}
                max={100}
                step={5}
                value={config.gameChatBalance}
                onChange={(e) =>
                  onChange({ ...config, gameChatBalance: Number(e.target.value) })
                }
                className="lumen-slider mt-2 w-full"
              />
              <div className="mt-1 flex justify-between text-[11px] text-[#84909D]">
                <span>专注游戏音效</span>
                <span>专注团队语音</span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
