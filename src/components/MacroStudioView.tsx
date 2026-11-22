import { useEffect, useRef, useState } from 'react';
import {
  ArrowDown,
  ArrowUp,
  Circle,
  Copy,
  Play,
  Plus,
  Square,
  Trash2,
} from 'lucide-react';
import type {
  MacroDefinition,
  MacroEventKind,
  MacroPlaybackMode,
  MacroStep,
} from '../types/peripheral';
import {
  calculateMacroDuration,
  normalizeMacroDelays,
  validateMacroName,
} from '../utils/deviceEngine';

interface MacroStudioViewProps {
  macros: MacroDefinition[];
  onChangeMacros: (next: MacroDefinition[]) => void;
  onSimulateRun: (macro: MacroDefinition) => void;
}

const PLAYBACK_MODES: { id: MacroPlaybackMode; label: string }[] = [
  { id: 'once', label: '单次执行' },
  { id: 'while-held', label: '按住循环' },
  { id: 'toggle', label: '切换循环' },
  { id: 'repeat', label: '指定次数' },
];

const EVENT_KIND_LABEL: Record<MacroEventKind, string> = {
  'key-down': '按下按键',
  'key-up': '抬起按键',
  'mouse-click': '鼠标点击',
  delay: '纯延迟等待',
};

export function MacroStudioView({
  macros,
  onChangeMacros,
  onSimulateRun,
}: MacroStudioViewProps) {
  const [selectedId, setSelectedId] = useState<string>(macros[0]?.id ?? '');
  const [newName, setNewName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [manualKey, setManualKey] = useState('KeyE');
  const [manualKind, setManualKind] = useState<MacroEventKind>('key-down');
  const [manualDelay, setManualDelay] = useState(25);
  const [runningStepIdx, setRunningStepIdx] = useState<number | null>(null);

  const lastRecordTimeRef = useRef<number>(0);
  const dryRunTimersRef = useRef<number[]>([]);

  const selectedMacro = macros.find((m) => m.id === selectedId) ?? macros[0] ?? null;

  const updateSelected = (updater: (m: MacroDefinition) => MacroDefinition) => {
    if (!selectedMacro) return;
    onChangeMacros(
      macros.map((m) =>
        m.id === selectedMacro.id
          ? { ...updater(m), updatedAt: new Date().toISOString().slice(0, 10) }
          : m
      )
    );
  };

  // Live Keyboard Recorder
  useEffect(() => {
    if (!isRecording || !selectedMacro) return;

    lastRecordTimeRef.current = performance.now();

    const handleKey = (e: KeyboardEvent, kind: 'key-down' | 'key-up') => {
      if (e.repeat) return;
      e.preventDefault();
      const now = performance.now();
      const delta = Math.max(5, Math.min(2000, Math.round(now - lastRecordTimeRef.current)));
      lastRecordTimeRef.current = now;

      const label = e.key === ' ' ? 'Space' : e.key.length === 1 ? e.key.toUpperCase() : e.key;
      const nextStep: MacroStep = {
        id: `st-${Date.now()}-${Math.random().toString(16).slice(2, 6)}`,
        kind,
        keyLabel: label,
        delayMs: delta,
      };
      updateSelected((m) => ({
        ...m,
        steps: [...m.steps, nextStep],
      }));
    };

    const onDown = (e: KeyboardEvent) => handleKey(e, 'key-down');
    const onUp = (e: KeyboardEvent) => handleKey(e, 'key-up');

    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    return () => {
      window.removeEventListener('keydown', onDown);
      window.removeEventListener('keyup', onUp);
    };
  }, [isRecording, selectedMacro?.id, macros]);

  useEffect(() => {
    return () => {
      dryRunTimersRef.current.forEach((t) => window.clearTimeout(t));
    };
  }, []);

  const handleCreateMacro = (e: React.FormEvent) => {
    e.preventDefault();
    const err = validateMacroName(
      newName,
      macros.map((m) => m.name)
    );
    if (err) {
      setCreateError(err);
      return;
    }
    const created: MacroDefinition = {
      id: `macro-${Date.now()}`,
      name: newName.trim(),
      description: '自定义键鼠自动化连招序列',
      playbackMode: 'once',
      repeatCount: 1,
      updatedAt: new Date().toISOString().slice(0, 10),
      steps: [
        { id: `st-${Date.now()}-1`, kind: 'key-down', keyLabel: 'E', delayMs: 20 },
        { id: `st-${Date.now()}-2`, kind: 'key-up', keyLabel: 'E', delayMs: 0 },
      ],
    };
    onChangeMacros([created, ...macros]);
    setSelectedId(created.id);
    setNewName('');
    setCreateError(null);
  };

  const handleDuplicate = (macro: MacroDefinition) => {
    const copyName = `${macro.name} 副本`;
    const duplicated: MacroDefinition = {
      ...macro,
      id: `macro-${Date.now()}`,
      name: copyName,
      updatedAt: new Date().toISOString().slice(0, 10),
      steps: macro.steps.map((s, idx) => ({ ...s, id: `st-${Date.now()}-${idx}` })),
    };
    onChangeMacros([duplicated, ...macros]);
    setSelectedId(duplicated.id);
  };

  const handleDelete = (id: string) => {
    const next = macros.filter((m) => m.id !== id);
    onChangeMacros(next);
    if (selectedId === id) {
      setSelectedId(next[0]?.id ?? '');
    }
  };

  const handleAddManualStep = () => {
    if (!selectedMacro) return;
    const label = manualKind === 'delay' ? '等待' : manualKey.trim() || 'Space';
    const step: MacroStep = {
      id: `st-${Date.now()}-${Math.random().toString(16).slice(2, 5)}`,
      kind: manualKind,
      keyLabel: label,
      delayMs: Math.max(0, Math.min(5000, manualDelay)),
    };
    updateSelected((m) => ({ ...m, steps: [...m.steps, step] }));
  };

  const handleMoveStep = (index: number, delta: -1 | 1) => {
    if (!selectedMacro) return;
    const target = index + delta;
    if (target < 0 || target >= selectedMacro.steps.length) return;
    const nextSteps = [...selectedMacro.steps];
    const [moved] = nextSteps.splice(index, 1);
    nextSteps.splice(target, 0, moved);
    updateSelected((m) => ({ ...m, steps: nextSteps }));
  };

  const handleDryRun = () => {
    if (!selectedMacro || selectedMacro.steps.length === 0) return;
    dryRunTimersRef.current.forEach((t) => window.clearTimeout(t));
    dryRunTimersRef.current = [];
    onSimulateRun(selectedMacro);

    let elapsed = 0;
    selectedMacro.steps.forEach((step, idx) => {
      const timer = window.setTimeout(() => {
        setRunningStepIdx(idx);
      }, elapsed);
      dryRunTimersRef.current.push(timer);
      elapsed += Math.max(120, Math.min(450, step.delayMs || 150));
    });
    const clearTimer = window.setTimeout(() => {
      setRunningStepIdx(null);
    }, elapsed + 160);
    dryRunTimersRef.current.push(clearTimer);
  };

  const totalDuration = selectedMacro ? calculateMacroDuration(selectedMacro.steps) : 0;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight text-[#181C20]">宏编排工作台</h1>
        <p className="mt-1 text-xs text-[#586370]">
          支持毫秒级键鼠动作录制、延迟标准化与本地预演，创建后可直接绑定至鼠标侧键或键盘任意层。
        </p>
      </div>

      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        {/* Left: Macro Library */}
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-5 xl:col-span-4 flex flex-col">
          <h2 className="text-sm font-semibold text-[#181C20]">宏方案库 ({macros.length})</h2>

          <form onSubmit={handleCreateMacro} className="mt-3 space-y-2">
            <div className="flex gap-2">
              <input
                type="text"
                value={newName}
                onChange={(e) => {
                  setNewName(e.target.value);
                  setCreateError(null);
                }}
                placeholder="输入新宏名称..."
                aria-label="新建宏名称"
                className="flex-1 min-w-0 rounded-xl border border-[#E5E9EE] px-3 py-2 text-xs text-[#181C20] placeholder:text-[#84909D] focus:border-[#2B6B68] focus:outline-none"
              />
              <button
                type="submit"
                className="inline-flex items-center gap-1 rounded-xl bg-[#2B6B68] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap shrink-0"
              >
                <Plus className="h-3.5 w-3.5" />
                <span>新建</span>
              </button>
            </div>
            {createError && (
              <p role="alert" className="text-xs text-[#B91C1C]">
                {createError}
              </p>
            )}
          </form>

          <ul className="mt-4 space-y-2 flex-1">
            {macros.length === 0 ? (
              <li className="rounded-xl border border-dashed border-[#D3DAE2] px-4 py-8 text-center text-xs text-[#84909D]">
                暂无已保存的宏方案，请在上方输入名称创建。
              </li>
            ) : (
              macros.map((m) => {
                const active = selectedMacro?.id === m.id;
                return (
                  <li
                    key={m.id}
                    className={`rounded-xl border p-3 transition-colors ${
                      active
                        ? 'border-[#2B6B68] bg-[#EBF3F2]/70'
                        : 'border-[#E5E9EE] bg-[#FAFBFD] hover:border-[#C9D3DC]'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <button
                        type="button"
                        onClick={() => {
                          setIsRecording(false);
                          setSelectedId(m.id);
                        }}
                        className="flex-1 text-left min-w-0"
                      >
                        <div className="truncate text-xs font-semibold text-[#181C20]">
                          {m.name}
                        </div>
                        <div className="mt-1 flex items-center gap-1.5 font-mono text-[11px] tabular-nums text-[#586370]">
                          <span>{m.steps.length} 步</span>
                          <span aria-hidden="true">·</span>
                          <span>{calculateMacroDuration(m.steps)} ms</span>
                        </div>
                      </button>

                      <div className="flex items-center gap-1 shrink-0">
                        <button
                          type="button"
                          onClick={() => handleDuplicate(m)}
                          aria-label={`复制宏 ${m.name}`}
                          className="rounded-md p-1.5 text-[#586370] hover:bg-white hover:text-[#181C20]"
                        >
                          <Copy className="h-3.5 w-3.5" />
                        </button>
                        <button
                          type="button"
                          onClick={() => handleDelete(m.id)}
                          aria-label={`删除宏 ${m.name}`}
                          className="rounded-md p-1.5 text-[#586370] hover:bg-white hover:text-[#B91C1C]"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </div>
                  </li>
                );
              })
            )}
          </ul>
        </section>

        {/* Right: Timeline & Recorder */}
        <section className="rounded-2xl border border-[#E5E9EE] bg-white p-6 xl:col-span-8">
          {!selectedMacro ? (
            <div className="py-16 text-center text-sm text-[#84909D]">
              请在左侧选择或新建一个宏方案开始编辑。
            </div>
          ) : (
            <div className="space-y-5">
              {/* Macro Meta & Actions */}
              <div className="flex flex-col gap-3 border-b border-[#EEF1F4] pb-4 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <h2 className="text-base font-semibold text-[#181C20]">{selectedMacro.name}</h2>
                  <p className="mt-0.5 text-xs text-[#586370]">
                    {selectedMacro.description} · 总耗时{' '}
                    <span className="font-mono font-semibold tabular-nums text-[#181C20]">
                      {totalDuration} ms
                    </span>
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setIsRecording((v) => !v)}
                    className={`inline-flex items-center gap-1.5 rounded-lg px-3.5 py-2 text-xs font-medium transition-colors whitespace-nowrap ${
                      isRecording
                        ? 'bg-[#B91C1C] text-white hover:bg-[#991B1B]'
                        : 'border border-[#E5E9EE] bg-white text-[#181C20] hover:bg-[#F0F3F6]'
                    }`}
                  >
                    {isRecording ? (
                      <>
                        <Square className="h-3.5 w-3.5" />
                        <span>停止录制 (监听按键中)</span>
                      </>
                    ) : (
                      <>
                        <Circle className="h-3.5 w-3.5 text-[#B91C1C]" />
                        <span>开始录制键盘</span>
                      </>
                    )}
                  </button>

                  <button
                    type="button"
                    onClick={handleDryRun}
                    disabled={selectedMacro.steps.length === 0}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-[#2B6B68] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#215452] disabled:opacity-40 transition-colors whitespace-nowrap"
                  >
                    <Play className="h-3.5 w-3.5" />
                    <span>模拟预演</span>
                  </button>
                </div>
              </div>

              {/* Playback Mode + Batch Delay Tools */}
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="inline-flex flex-wrap items-center gap-1 rounded-xl bg-[#EEF1F4] p-1">
                  {PLAYBACK_MODES.map((mode) => (
                    <button
                      key={mode.id}
                      type="button"
                      onClick={() => updateSelected((m) => ({ ...m, playbackMode: mode.id }))}
                      className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors whitespace-nowrap ${
                        selectedMacro.playbackMode === mode.id
                          ? 'bg-white text-[#181C20] shadow-xs'
                          : 'text-[#586370] hover:text-[#181C20]'
                      }`}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>

                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-[11px] text-[#84909D] mr-1">批量延迟：</span>
                  <button
                    type="button"
                    onClick={() =>
                      updateSelected((m) => ({
                        ...m,
                        steps: normalizeMacroDelays(m.steps, 'fixed', 25),
                      }))
                    }
                    className="rounded-lg bg-[#F0F3F6] px-2.5 py-1 text-[11px] font-medium text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20] whitespace-nowrap"
                  >
                    统一 25ms
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      updateSelected((m) => ({
                        ...m,
                        steps: normalizeMacroDelays(m.steps, 'scale', 0.5),
                      }))
                    }
                    className="rounded-lg bg-[#F0F3F6] px-2.5 py-1 text-[11px] font-medium text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20] whitespace-nowrap"
                  >
                    提速 2x
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      updateSelected((m) => ({
                        ...m,
                        steps: normalizeMacroDelays(m.steps, 'zero', 5),
                      }))
                    }
                    className="rounded-lg bg-[#F0F3F6] px-2.5 py-1 text-[11px] font-medium text-[#586370] hover:bg-[#E5E9EE] hover:text-[#181C20] whitespace-nowrap"
                  >
                    极速 5ms
                  </button>
                </div>
              </div>

              {/* Manual Insert Bar */}
              <div className="grid grid-cols-1 gap-2 rounded-xl bg-[#F7F8FA] p-3 sm:grid-cols-12">
                <select
                  aria-label="事件类型"
                  value={manualKind}
                  onChange={(e) => setManualKind(e.target.value as MacroEventKind)}
                  className="sm:col-span-3 rounded-lg border border-[#E5E9EE] bg-white px-2.5 py-1.5 text-xs text-[#181C20]"
                >
                  <option value="key-down">按下按键</option>
                  <option value="key-up">抬起按键</option>
                  <option value="mouse-click">鼠标点击</option>
                  <option value="delay">纯延迟等待</option>
                </select>
                <input
                  type="text"
                  aria-label="按键名称"
                  value={manualKey}
                  disabled={manualKind === 'delay'}
                  onChange={(e) => setManualKey(e.target.value)}
                  placeholder="键名 (如 Q / Ctrl / 左键)"
                  className="sm:col-span-4 rounded-lg border border-[#E5E9EE] bg-white px-3 py-1.5 text-xs text-[#181C20] disabled:opacity-50"
                />
                <div className="sm:col-span-3 flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-2.5 py-1.5">
                  <input
                    type="number"
                    aria-label="步骤后延迟毫秒"
                    min={0}
                    max={5000}
                    value={manualDelay}
                    onChange={(e) => setManualDelay(Number(e.target.value))}
                    className="w-full font-mono text-xs tabular-nums text-[#181C20] focus:outline-none"
                  />
                  <span className="text-[11px] text-[#84909D]">ms</span>
                </div>
                <button
                  type="button"
                  onClick={handleAddManualStep}
                  className="sm:col-span-2 rounded-lg bg-[#181C20] px-3 py-1.5 text-xs font-medium text-white hover:bg-[#2C333B] whitespace-nowrap"
                >
                  插入步骤
                </button>
              </div>

              {/* Event Timeline Table */}
              <div className="overflow-hidden rounded-xl border border-[#E5E9EE]">
                <table className="w-full text-left text-xs">
                  <thead className="bg-[#F7F8FA] text-[#586370]">
                    <tr>
                      <th className="px-3.5 py-2.5 font-medium w-12">#</th>
                      <th className="px-3.5 py-2.5 font-medium">动作类型</th>
                      <th className="px-3.5 py-2.5 font-medium">触发键值</th>
                      <th className="px-3.5 py-2.5 font-medium text-right">步后延迟</th>
                      <th className="px-3.5 py-2.5 font-medium text-right w-28">操作</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-[#EEF1F4]">
                    {selectedMacro.steps.length === 0 ? (
                      <tr>
                        <td colSpan={5} className="px-4 py-8 text-center text-[#84909D]">
                          暂无步骤，点击“开始录制键盘”或在上方手动插入动作。
                        </td>
                      </tr>
                    ) : (
                      selectedMacro.steps.map((step, idx) => {
                        const isExecuting = runningStepIdx === idx;
                        return (
                          <tr
                            key={step.id}
                            className={`transition-colors ${
                              isExecuting ? 'bg-[#EBF3F2]' : 'bg-white hover:bg-[#FAFBFD]'
                            }`}
                          >
                            <td className="px-3.5 py-2.5 font-mono tabular-nums text-[#84909D]">
                              {String(idx + 1).padStart(2, '0')}
                            </td>
                            <td className="px-3.5 py-2.5 text-[#586370]">
                              {EVENT_KIND_LABEL[step.kind]}
                            </td>
                            <td className="px-3.5 py-2.5 font-mono font-medium text-[#181C20]">
                              {step.keyLabel}
                            </td>
                            <td className="px-3.5 py-2.5 text-right">
                              <input
                                type="number"
                                aria-label={`第 ${idx + 1} 步延迟毫秒`}
                                min={0}
                                max={5000}
                                value={step.delayMs}
                                onChange={(e) => {
                                  const nextVal = Math.max(0, Math.min(5000, Number(e.target.value)));
                                  updateSelected((m) => ({
                                    ...m,
                                    steps: m.steps.map((s, i) =>
                                      i === idx ? { ...s, delayMs: nextVal } : s
                                    ),
                                  }));
                                }}
                                className="w-16 rounded-md border border-[#E5E9EE] px-2 py-1 text-right font-mono text-xs tabular-nums text-[#181C20]"
                              />
                              <span className="ml-1 text-[11px] text-[#84909D]">ms</span>
                            </td>
                            <td className="px-3.5 py-2.5 text-right">
                              <div className="inline-flex items-center gap-0.5">
                                <button
                                  type="button"
                                  disabled={idx === 0}
                                  onClick={() => handleMoveStep(idx, -1)}
                                  aria-label={`上移第 ${idx + 1} 步`}
                                  className="rounded p-1 text-[#586370] hover:bg-[#F0F3F6] disabled:opacity-30"
                                >
                                  <ArrowUp className="h-3.5 w-3.5" />
                                </button>
                                <button
                                  type="button"
                                  disabled={idx === selectedMacro.steps.length - 1}
                                  onClick={() => handleMoveStep(idx, 1)}
                                  aria-label={`下移第 ${idx + 1} 步`}
                                  className="rounded p-1 text-[#586370] hover:bg-[#F0F3F6] disabled:opacity-30"
                                >
                                  <ArrowDown className="h-3.5 w-3.5" />
                                </button>
                                <button
                                  type="button"
                                  onClick={() =>
                                    updateSelected((m) => ({
                                      ...m,
                                      steps: m.steps.filter((_, i) => i !== idx),
                                    }))
                                  }
                                  aria-label={`删除第 ${idx + 1} 步`}
                                  className="rounded p-1 text-[#586370] hover:bg-[#F0F3F6] hover:text-[#B91C1C]"
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                </button>
                              </div>
                            </td>
                          </tr>
                        );
                      })
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
