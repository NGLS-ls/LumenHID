import { useCallback, useEffect, useRef, useState } from 'react';
import { ArrowUpCircle, Lock, Menu, Save, Unlock } from 'lucide-react';
import { Sidebar } from './components/Sidebar';
import { DevicesView } from './components/DevicesView';
import { MouseView } from './components/MouseView';
import { KeyboardView } from './components/KeyboardView';
import { MacroStudioView } from './components/MacroStudioView';
import { AudioStudioView } from './components/AudioStudioView';
import { UpdateModal } from './components/UpdateModal';
import {
  DEFAULT_AUDIO_CONFIG,
  DEFAULT_KEYBOARD_CONFIG,
  DEFAULT_MOUSE_CONFIG,
  INITIAL_MACROS,
} from './data/seedDevices';
import type {
  AppProfileSnapshot,
  AudioConfig,
  KeyboardConfig,
  MacroDefinition,
  MouseConfig,
  NavSection,
  PeripheralDevice,
} from './types/peripheral';
import { parseAndValidateProfile, serializeProfile } from './utils/deviceEngine';
import {
  applyHeadsetLed,
  checkAppUpdate,
  downloadAndApplyUpdate,
  downloadTextFile,
  emergencyResetKeyboardEngine,
  getKeyboardEngineStatus,
  getSafetyStatus,
  getThxStatus,
  getUpdaterSettings,
  isTauriRuntime,
  loadSavedProfile,
  openExternalUrl,
  refreshNativeBatteries,
  rollbackAudioConfig,
  rollbackKeyboardConfig,
  rollbackMouseConfig,
  saveProfileSnapshot,
  saveUpdaterSettings,
  scanNativeWindowsDevices,
  setKeyboardEngineEnabled,
  setReadonlyMode,
  syncAudioHardware,
  syncKeyboardHardware,
  syncMacrosToEngine,
  syncMouseHardware,
  triggerMacroHardware,
  type KeyboardEngineStatus,
  type SafetyStatus,
  type TelemetryEventPayload,
  type ThxStatus,
  type UpdateCheckResult,
  type UpdateProgressPayload,
  type UpdaterSettingsView,
} from './services/deviceBridge';

const SECTION_BREADCRUMBS: Record<NavSection, string> = {
  devices: '设备总览',
  mouse: '鼠标调校',
  keyboard: '键盘与轴体',
  macros: '宏工作台',
  audio: '音频与声场',
};

export default function App() {
  const initialSnapshot = loadSavedProfile();

  const [activeSection, setActiveSection] = useState<NavSection>('devices');
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const [scanning, setScanning] = useState(false);

  const [devices, setDevices] = useState<PeripheralDevice[]>(
    initialSnapshot?.devices ?? []
  );
  const [activeDeviceId, setActiveDeviceId] = useState<string>(
    initialSnapshot?.activeDeviceId ?? ''
  );
  const [mouseConfig, setMouseConfig] = useState<MouseConfig>(
    initialSnapshot?.mouseConfig ?? DEFAULT_MOUSE_CONFIG
  );
  const [keyboardConfig, setKeyboardConfig] = useState<KeyboardConfig>(
    initialSnapshot?.keyboardConfig ?? DEFAULT_KEYBOARD_CONFIG
  );
  const [macros, setMacros] = useState<MacroDefinition[]>(
    initialSnapshot?.macros ?? INITIAL_MACROS
  );
  const [audioConfig, setAudioConfig] = useState<AudioConfig>(
    initialSnapshot?.audioConfig ?? DEFAULT_AUDIO_CONFIG
  );

  const [notice, setNotice] = useState<string | null>(null);
  const [keyboardEngineStatus, setKeyboardEngineStatus] = useState<KeyboardEngineStatus | null>(
    null
  );
  const [thxStatus, setThxStatus] = useState<ThxStatus | null>(null);
  const [safetyStatus, setSafetyStatus] = useState<SafetyStatus | null>(null);
  const [updaterSettings, setUpdaterSettings] = useState<UpdaterSettingsView | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [updateModalOpen, setUpdateModalOpen] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updatingApp, setUpdatingApp] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgressPayload | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const saveTimerRef = useRef<number | null>(null);

  const showNotice = useCallback((msg: string) => {
    setNotice(msg);
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 2800);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const mergeScannedDevices = useCallback((found: PeripheralDevice[]) => {
    setDevices(found);
    setActiveDeviceId((prev) => {
      if (prev && found.some((d) => d.id === prev)) return prev;
      return found[0]?.id ?? '';
    });
  }, []);

  // Auto-discover real Windows native HID & Bluetooth PnP devices asynchronously on launch
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    setScanning(true);
    void scanNativeWindowsDevices().then((nativeDevs) => {
      if (cancelled) return;
      mergeScannedDevices(nativeDevs);
      setScanning(false);
    });
    return () => {
      cancelled = true;
    };
  }, [mergeScannedDevices]);

  // Sync macros & initial keyboard soft-driver / THX status in Tauri desktop runtime
  useEffect(() => {
    if (!isTauriRuntime()) return;
    void syncMacrosToEngine(macros);
  }, [macros]);

  const refreshSafetyStatus = useCallback(() => {
    if (!isTauriRuntime()) return;
    void getSafetyStatus().then((st) => {
      if (st) setSafetyStatus(st);
    });
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void getThxStatus().then((st) => {
      if (st) setThxStatus(st);
    });
    refreshSafetyStatus();
  }, [activeSection, refreshSafetyStatus]);

  const handleCheckUpdate = useCallback(async (silent = false) => {
    if (!isTauriRuntime()) {
      if (!silent) {
        setUpdateError('当前处于 Web 预览模式，请在 Windows 桌面端运行在线更新检查');
      }
      return;
    }
    setCheckingUpdate(true);
    setUpdateError(null);
    const res = await checkAppUpdate();
    setCheckingUpdate(false);
    if (res.ok) {
      setUpdateInfo(res.data);
      if (!silent && !res.data.has_update) {
        showNotice(res.data.detail);
      }
    } else {
      setUpdateInfo((prev) => (prev?.auth_required ? null : prev));
      setUpdateError(res.error);
      if (!silent) {
        showNotice(res.error);
      }
    }
  }, [showNotice]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void getUpdaterSettings().then((st) => {
      if (st) setUpdaterSettings(st);
    });
    void handleCheckUpdate(true);

    const onUpdateProgress = (ev: Event) => {
      const detail = (ev as CustomEvent<UpdateProgressPayload>).detail;
      if (detail) {
        setUpdateProgress(detail);
      }
    };
    window.addEventListener('lumen-update-progress', onUpdateProgress);
    return () => {
      window.removeEventListener('lumen-update-progress', onUpdateProgress);
    };
  }, [handleCheckUpdate]);

  const handleApplyUpdate = async () => {
    if (!updateInfo) return;
    setUpdatingApp(true);
    setUpdateError(null);
    setUpdateProgress({
      stage: 'downloading',
      downloaded_bytes: 0,
      total_bytes: updateInfo.asset_size,
      percent: 0,
      message: '正在准备下载安装包...',
    });

    const res = await downloadAndApplyUpdate(updateInfo);
    setUpdatingApp(false);
    if (res.ok) {
      showNotice(res.data.detail);
    } else {
      setUpdateError(res.error);
      showNotice(res.error);
    }
  };

  const handleSaveUpdaterSettings = async (
    repo: string,
    token: string | null,
    clearToken: boolean
  ): Promise<boolean> => {
    const res = await saveUpdaterSettings(repo, token, clearToken);
    if (res.ok) {
      setUpdaterSettings(res.data);
      setUpdateError(null);
      showNotice('已保存 GitHub 更新源设置');
      return true;
    }
    setUpdateError(res.error);
    return false;
  };

  const handleOpenReleaseUrl = (url: string) => {
    void openExternalUrl(url).then((res) => {
      if (!res.ok) {
        showNotice(res.error);
      }
    });
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void syncKeyboardHardware(keyboardConfig, null).then(() => {
      void getKeyboardEngineStatus().then((st) => {
        if (st) setKeyboardEngineStatus(st);
      });
    });

    const onKbdStatus = (ev: Event) => {
      const detail = (ev as CustomEvent<KeyboardEngineStatus>).detail;
      if (!detail) return;
      setKeyboardEngineStatus(detail);
      if (detail.last_notice) {
        showNotice(detail.last_notice);
      }
    };
    window.addEventListener('lumen-keyboard-status', onKbdStatus);
    return () => {
      window.removeEventListener('lumen-keyboard-status', onKbdStatus);
    };
  }, [showNotice]);

  // Periodic light battery refresh every 15s + instant passive telemetry listener in desktop runtime
  useEffect(() => {
    if (!isTauriRuntime()) return;

    const handleTelemetry = (ev: Event) => {
      const detail = (ev as CustomEvent<TelemetryEventPayload>).detail;
      if (!detail) return;
      setDevices((prev) =>
        prev.map((dev) => {
          if (dev.vendorId !== detail.vendor_id || dev.productId !== detail.product_id) {
            return dev;
          }
          return {
            ...dev,
            name: detail.product_name || dev.name,
            batteryPercent: detail.battery_percent ?? dev.batteryPercent,
            charging: detail.charging,
            batterySource:
              detail.battery_percent !== null ? detail.battery_source : dev.batterySource,
            firmwareVersion: detail.firmware_version || dev.firmwareVersion,
          };
        })
      );
      if (typeof detail.active_dpi_stage === 'number') {
        const stageIdx = detail.active_dpi_stage;
        setMouseConfig((prev) =>
          stageIdx >= 0 && stageIdx < prev.dpiStages.length && prev.activeStageIndex !== stageIdx
            ? { ...prev, activeStageIndex: stageIdx }
            : prev
        );
      }
    };

    window.addEventListener('lumen-telemetry', handleTelemetry);

    const interval = window.setInterval(() => {
      void refreshNativeBatteries().then((updates) => {
        if (updates.length === 0) return;
        setDevices((prev) =>
          prev.map((dev) => {
            const hit = updates.find(
              (u) =>
                (u.vendor_id === dev.vendorId && u.product_id === dev.productId) ||
                u.product_name === dev.name
            );
            if (!hit) return dev;
            return {
              ...dev,
              name: hit.product_name || dev.name,
              batteryPercent: hit.battery_percent ?? dev.batteryPercent,
              charging: hit.charging || hit.battery_source === 'wired',
              batterySource: hit.battery_percent !== null ? hit.battery_source : dev.batterySource,
              firmwareVersion: hit.firmware_version || dev.firmwareVersion,
            };
          })
        );
      });
    }, 15000);
    return () => {
      window.removeEventListener('lumen-telemetry', handleTelemetry);
      window.clearInterval(interval);
    };
  }, []);

  // Debounced auto-persist (450ms) so dragging sliders never blocks UI frames or disk I/O
  useEffect(() => {
    if (saveTimerRef.current !== null) {
      window.clearTimeout(saveTimerRef.current);
    }
    saveTimerRef.current = window.setTimeout(() => {
      const snapshot: AppProfileSnapshot = {
        schemaVersion: '2.0.0',
        exportedAt: new Date().toISOString(),
        activeDeviceId,
        devices,
        mouseConfig,
        keyboardConfig,
        macros,
        audioConfig,
      };
      void saveProfileSnapshot(snapshot);
    }, 450);
    return () => {
      if (saveTimerRef.current !== null) {
        window.clearTimeout(saveTimerRef.current);
      }
    };
  }, [activeDeviceId, devices, mouseConfig, keyboardConfig, macros, audioConfig]);

  const mouseDevice =
    devices.find((d) => d.category === 'mouse' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'mouse') ??
    null;

  const keyboardDevice =
    devices.find((d) => d.category === 'keyboard' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'keyboard') ??
    null;

  const headsetDevice =
    devices.find((d) => d.category === 'headset' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'headset') ??
    null;

  const handleSelectDevice = (device: PeripheralDevice) => {
    setActiveDeviceId(device.id);
    if (device.category === 'mouse') setActiveSection('mouse');
    else if (device.category === 'keyboard') setActiveSection('keyboard');
    else setActiveSection('audio');
  };

  const handleRefreshDevices = async () => {
    if (!isTauriRuntime()) {
      showNotice('桌面便携版 EXE 会自动扫描本机 2.4G / 蓝牙 / USB 外设并读取真实电量');
      return;
    }
    setScanning(true);
    const found = await scanNativeWindowsDevices();
    mergeScannedDevices(found);
    setScanning(false);
    showNotice(
      found.length > 0
        ? `已刷新 ${found.length} 台本机物理外设与实时电量`
        : '未扫描到外接键鼠或蓝牙耳机（已过滤系统音频端点）'
    );
  };

  const handleMouseChange = (next: MouseConfig) => {
    setMouseConfig(next);
    if (mouseDevice) {
      setDevices((prev) =>
        prev.map((d) =>
          d.id === mouseDevice.id ? { ...d, pollingRateHz: next.pollingRateHz } : d
        )
      );
      syncMouseHardware(next, mouseDevice);
    }
  };

  const handleRollbackMouse = () => {
    void rollbackMouseConfig(mouseDevice).then((res) => {
      if (!res.ok) {
        showNotice(res.error);
        return;
      }
      const restored = res.data.mouseConfig;
      if (restored) {
        setMouseConfig(restored);
        if (mouseDevice) {
          setDevices((prev) =>
            prev.map((d) =>
              d.id === mouseDevice.id ? { ...d, pollingRateHz: restored.pollingRateHz } : d
            )
          );
        }
      }
      showNotice(res.data.detail);
    });
  };

  const handleKeyboardChange = (next: KeyboardConfig) => {
    setKeyboardConfig(next);
    if (keyboardDevice) {
      setDevices((prev) =>
        prev.map((d) =>
          d.id === keyboardDevice.id ? { ...d, pollingRateHz: next.pollingRateHz } : d
        )
      );
    }
    void syncKeyboardHardware(next, keyboardDevice).then(() => {
      void getKeyboardEngineStatus().then((st) => {
        if (st) setKeyboardEngineStatus(st);
      });
      refreshSafetyStatus();
    });
  };

  const handleToggleReadonlyMode = (enabled: boolean) => {
    void setReadonlyMode(enabled).then((st) => {
      if (st) {
        setSafetyStatus(st);
        showNotice(
          st.readonly_mode
            ? '已开启全局只读安全模式：所有硬件写操作、THX 写入与键盘拦截均已锁定'
            : '已解除只读安全模式：已放行白名单设备的防变砖安全写通道'
        );
      }
    });
  };

  const handleToggleKeyboardEngine = (enabled: boolean) => {
    void setKeyboardEngineEnabled(enabled).then((st) => {
      if (st) {
        setKeyboardEngineStatus(st);
        if (st.last_notice) showNotice(st.last_notice);
      }
    });
  };

  const handleEmergencyStopKeyboard = () => {
    void emergencyResetKeyboardEngine().then((st) => {
      if (st) {
        setKeyboardEngineStatus(st);
        if (st.last_notice) showNotice(st.last_notice);
      }
    });
  };

  const handleRollbackKeyboard = () => {
    void rollbackKeyboardConfig().then((restored) => {
      if (restored) {
        setKeyboardConfig(restored);
        void getKeyboardEngineStatus().then((st) => {
          if (st) setKeyboardEngineStatus(st);
        });
        showNotice('已成功回滚至上一份键盘软驱动配置快照');
      } else {
        showNotice('暂无可回滚的键盘配置历史快照');
      }
    });
  };

  const handleAudioChange = (next: AudioConfig) => {
    setAudioConfig(next);
    syncAudioHardware(next, headsetDevice, () => {
      refreshSafetyStatus();
    });
  };

  const handleRollbackAudio = () => {
    void rollbackAudioConfig().then((res) => {
      if (!res.ok) {
        showNotice(res.error);
        return;
      }
      const { uiConfig, thxSnapshot, detail } = res.data;
      setAudioConfig((prev) => {
        const base = uiConfig ?? prev;
        if (!thxSnapshot) return base;
        return {
          ...base,
          activePresetId:
            thxSnapshot.eq_bands.every((b) => Math.abs(b) < 0.01) && !thxSnapshot.eq_enabled
              ? 'flat'
              : base.activePresetId,
          bands: thxSnapshot.eq_bands.length === 10 ? thxSnapshot.eq_bands : base.bands,
          surround71Enabled: thxSnapshot.spatial_enabled,
          bassBoost: thxSnapshot.bassboost_enabled
            ? Math.round(thxSnapshot.bassboost_level * 100)
            : 0,
          voiceClarity: thxSnapshot.voice_clarity_enabled
            ? Math.round(thxSnapshot.voice_clarity_level * 100)
            : 0,
        };
      });
      showNotice(detail);
    });
  };

  const handleHeadsetLedMode = (mode: number) => {
    if (!headsetDevice) return;
    void applyHeadsetLed(headsetDevice, mode).then((res) => {
      showNotice(res.ok ? res.data.detail : res.error);
    });
  };

  const handleExportProfile = () => {
    const snapshot: AppProfileSnapshot = {
      schemaVersion: '2.0.0',
      exportedAt: new Date().toISOString(),
      activeDeviceId,
      devices,
      mouseConfig,
      keyboardConfig,
      macros,
      audioConfig,
    };
    downloadTextFile(
      'LumenHID-portable-profile.json',
      serializeProfile(snapshot),
      'application/json;charset=utf-8'
    );
    showNotice('已导出便携配置文件 LumenHID-portable-profile.json');
  };

  const handleImportProfile = (rawJson: string): { ok: boolean; message: string } => {
    const result = parseAndValidateProfile(rawJson);
    if (!result.ok) {
      return { ok: false, message: result.error };
    }
    setDevices(result.data.devices);
    setActiveDeviceId(result.data.activeDeviceId);
    setMouseConfig(result.data.mouseConfig);
    setKeyboardConfig(result.data.keyboardConfig);
    setMacros(result.data.macros);
    setAudioConfig(result.data.audioConfig);
    showNotice('已成功导入便携配置文件');
    return { ok: true, message: '配置文件校验通过并已全部应用。' };
  };

  const handleWriteToHardware = () => {
    syncMouseHardware(mouseConfig, mouseDevice, (res) => {
      if (activeSection === 'mouse') {
        showNotice(res.detail);
      }
      refreshSafetyStatus();
    });
    void syncKeyboardHardware(keyboardConfig, keyboardDevice).then(() => {
      refreshSafetyStatus();
    });
    syncAudioHardware(audioConfig, headsetDevice, (res) => {
      if (activeSection === 'audio') {
        showNotice(res.detail);
      }
      refreshSafetyStatus();
    });
    const snapshot: AppProfileSnapshot = {
      schemaVersion: '2.0.0',
      exportedAt: new Date().toISOString(),
      activeDeviceId,
      devices,
      mouseConfig,
      keyboardConfig,
      macros,
      audioConfig,
    };
    void saveProfileSnapshot(snapshot);
    showNotice('已异步下发支持协议的硬件参数并写入便携目录');
  };

  return (
    <div className="flex min-h-screen bg-[#F6F7F9] text-[#181C20]">
      <Sidebar
        activeSection={activeSection}
        onSelectSection={setActiveSection}
        devices={devices}
        activeDeviceId={activeDeviceId}
        onSelectDevice={handleSelectDevice}
        mobileOpen={mobileMenuOpen}
        onCloseMobile={() => setMobileMenuOpen(false)}
        appVersion={updateInfo?.current_version ?? updaterSettings?.current_version}
        updateInfo={updateInfo}
        checkingUpdate={checkingUpdate}
        onOpenUpdateModal={() => {
          setUpdateModalOpen(true);
          if (!updateInfo && !checkingUpdate) {
            void handleCheckUpdate(false);
          }
        }}
      />

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-30 flex h-16 items-center justify-between gap-4 border-b border-[#E5E9EE] bg-[#F6F7F9]/90 px-4 backdrop-blur-md sm:px-8">
          <div className="flex items-center gap-3 min-w-0">
            <button
              type="button"
              onClick={() => setMobileMenuOpen(true)}
              aria-label="打开导航菜单"
              className="lg:hidden rounded-lg p-2 text-[#586370] hover:bg-white hover:text-[#181C20]"
            >
              <Menu className="h-4 w-4" />
            </button>
            <nav aria-label="面包屑导航" className="flex items-center gap-2 text-xs truncate">
              <span className="text-[#84909D] whitespace-nowrap">LumenHID</span>
              <span aria-hidden="true" className="text-[#CBD3DC]">
                /
              </span>
              <span className="font-medium text-[#181C20] truncate">
                {SECTION_BREADCRUMBS[activeSection]}
              </span>
            </nav>
          </div>

          <div className="flex items-center gap-2.5 shrink-0">
            {updateInfo?.has_update && (
              <button
                type="button"
                onClick={() => setUpdateModalOpen(true)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-[#C4DCD9] bg-[#EBF3F2] px-3 py-2 text-xs font-medium text-[#1D4E4C] hover:bg-[#DCEBE9] transition-colors whitespace-nowrap shrink-0"
                title={updateInfo.detail}
              >
                <ArrowUpCircle className="h-3.5 w-3.5 text-[#2B6B68]" />
                <span>
                  {updatingApp && updateProgress
                    ? `升级中 ${updateProgress.percent}%`
                    : `升级到 v${updateInfo.latest_version ?? updateInfo.tag_name ?? ''}`}
                </span>
              </button>
            )}

            <button
              type="button"
              role="switch"
              aria-checked={Boolean(safetyStatus?.readonly_mode)}
              onClick={() => handleToggleReadonlyMode(!safetyStatus?.readonly_mode)}
              className={`hidden sm:inline-flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs font-medium transition-colors whitespace-nowrap ${
                safetyStatus?.readonly_mode
                  ? 'border-[#D97706] bg-[#FEF3C7] text-[#92400E] hover:bg-[#FDE68A]'
                  : 'border-[#DCE5E4] bg-white text-[#1D4E4C] hover:bg-[#EBF3F2]'
              }`}
              title="随时开启以全局禁止硬件写指令"
            >
              {safetyStatus?.readonly_mode ? (
                <Lock className="h-3.5 w-3.5" />
              ) : (
                <Unlock className="h-3.5 w-3.5" />
              )}
              <span>{safetyStatus?.readonly_mode ? '只读安全锁定' : '只读安全锁'}</span>
            </button>

            <button
              type="button"
              onClick={handleWriteToHardware}
              className="inline-flex items-center gap-1.5 rounded-lg bg-[#181C20] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#2C333B] transition-colors whitespace-nowrap shrink-0"
            >
              <Save className="h-3.5 w-3.5" />
              <span>保存并下发配置</span>
            </button>
          </div>
        </header>

        <main className="mx-auto w-full max-w-[1220px] flex-1 px-4 py-6 sm:px-8 sm:py-8">
          <div key={activeSection} className="lumen-view-enter">
            {activeSection === 'devices' && (
              <DevicesView
                devices={devices}
                activeDeviceId={activeDeviceId}
                scanning={scanning}
                safetyStatus={safetyStatus}
                keyboardEngineStatus={keyboardEngineStatus}
                onSelectDeviceAndOpen={(dev, sec) => {
                  setActiveDeviceId(dev.id);
                  setActiveSection(sec);
                }}
                onRefreshDevices={handleRefreshDevices}
                onMergeScannedDevices={mergeScannedDevices}
                onExportProfile={handleExportProfile}
                onImportProfile={handleImportProfile}
                onToggleReadonlyMode={handleToggleReadonlyMode}
                onEmergencyStopKeyboard={handleEmergencyStopKeyboard}
              />
            )}

            {activeSection === 'mouse' && (
              <MouseView
                device={mouseDevice}
                config={mouseConfig}
                macros={macros}
                onChange={handleMouseChange}
                onRollback={handleRollbackMouse}
                onReset={() => {
                  handleMouseChange(DEFAULT_MOUSE_CONFIG);
                  showNotice('鼠标参数已恢复出厂默认');
                }}
              />
            )}

            {activeSection === 'keyboard' && (
              <KeyboardView
                device={keyboardDevice}
                config={keyboardConfig}
                macros={macros}
                engineStatus={keyboardEngineStatus}
                onChange={handleKeyboardChange}
                onReset={() => {
                  handleKeyboardChange(DEFAULT_KEYBOARD_CONFIG);
                  showNotice('键盘键位与轴体参数已恢复默认');
                }}
                onToggleEngine={handleToggleKeyboardEngine}
                onEmergencyStop={handleEmergencyStopKeyboard}
                onRollback={handleRollbackKeyboard}
              />
            )}

            {activeSection === 'macros' && (
              <MacroStudioView
                macros={macros}
                onChangeMacros={setMacros}
                onSimulateRun={(m) => {
                  void triggerMacroHardware(m);
                  showNotice(`正在模拟执行宏：${m.name}`);
                }}
              />
            )}

            {activeSection === 'audio' && (
              <AudioStudioView
                device={headsetDevice}
                config={audioConfig}
                thxStatus={thxStatus}
                onChange={handleAudioChange}
                onRollback={handleRollbackAudio}
                onChangeLedMode={handleHeadsetLedMode}
                onReset={() => {
                  handleAudioChange({
                    ...DEFAULT_AUDIO_CONFIG,
                    activePresetId: 'flat',
                    bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                    preampDb: 0,
                  });
                  showNotice('均衡器曲线已重置为纯净原声');
                }}
                onExportApoNotice={(msg) => showNotice(msg)}
              />
            )}
          </div>
        </main>
      </div>

      <UpdateModal
        open={updateModalOpen}
        onClose={() => {
          if (!updatingApp) setUpdateModalOpen(false);
        }}
        settings={updaterSettings}
        updateInfo={updateInfo}
        checking={checkingUpdate}
        updating={updatingApp}
        progress={updateProgress}
        errorMessage={updateError}
        onCheckNow={() => void handleCheckUpdate(false)}
        onApplyUpdate={() => void handleApplyUpdate()}
        onSaveSettings={handleSaveUpdaterSettings}
        onOpenReleaseUrl={handleOpenReleaseUrl}
      />

      {notice && (
        <div
          role="status"
          aria-live="polite"
          className="lumen-toast-enter fixed bottom-5 right-5 z-50 rounded-xl border border-[#C4DCD9] bg-white px-4 py-2.5 text-xs font-medium text-[#1D4E4C] shadow-lg"
        >
          {notice}
        </div>
      )}
    </div>
  );
}
