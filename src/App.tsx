import { useCallback, useEffect, useRef, useState } from 'react';
import { Menu, Save } from 'lucide-react';
import { Sidebar } from './components/Sidebar';
import { DevicesView } from './components/DevicesView';
import { MouseView } from './components/MouseView';
import { KeyboardView } from './components/KeyboardView';
import { MacroStudioView } from './components/MacroStudioView';
import { AudioStudioView } from './components/AudioStudioView';
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
  downloadTextFile,
  isTauriRuntime,
  loadSavedProfile,
  refreshNativeBatteries,
  saveProfileSnapshot,
  scanNativeWindowsDevices,
  syncAudioHardware,
  syncKeyboardHardware,
  syncMouseHardware,
  triggerMacroHardware,
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

  // Periodic light battery refresh every 30s in desktop runtime (non-blocking async Rust)
  useEffect(() => {
    if (!isTauriRuntime()) return;
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
            if (!hit || hit.battery_percent === null) return dev;
            return {
              ...dev,
              batteryPercent: hit.battery_percent,
              batterySource: hit.battery_source,
            };
          })
        );
      });
    }, 30000);
    return () => window.clearInterval(interval);
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

  const handleKeyboardChange = (next: KeyboardConfig) => {
    setKeyboardConfig(next);
    if (keyboardDevice) {
      setDevices((prev) =>
        prev.map((d) =>
          d.id === keyboardDevice.id ? { ...d, pollingRateHz: next.pollingRateHz } : d
        )
      );
      syncKeyboardHardware(next, keyboardDevice);
    }
  };

  const handleAudioChange = (next: AudioConfig) => {
    setAudioConfig(next);
    syncAudioHardware(next, headsetDevice);
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
    syncMouseHardware(mouseConfig, mouseDevice);
    syncKeyboardHardware(keyboardConfig, keyboardDevice);
    syncAudioHardware(audioConfig, headsetDevice);
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

          <button
            type="button"
            onClick={handleWriteToHardware}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[#181C20] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#2C333B] transition-colors whitespace-nowrap shrink-0"
          >
            <Save className="h-3.5 w-3.5" />
            <span>保存并下发配置</span>
          </button>
        </header>

        <main className="mx-auto w-full max-w-[1220px] flex-1 px-4 py-6 sm:px-8 sm:py-8">
          <div key={activeSection} className="lumen-view-enter">
            {activeSection === 'devices' && (
              <DevicesView
                devices={devices}
                activeDeviceId={activeDeviceId}
                scanning={scanning}
                onSelectDeviceAndOpen={(dev, sec) => {
                  setActiveDeviceId(dev.id);
                  setActiveSection(sec);
                }}
                onRefreshDevices={handleRefreshDevices}
                onMergeScannedDevices={mergeScannedDevices}
                onExportProfile={handleExportProfile}
                onImportProfile={handleImportProfile}
              />
            )}

            {activeSection === 'mouse' && (
              <MouseView
                device={mouseDevice}
                config={mouseConfig}
                macros={macros}
                onChange={handleMouseChange}
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
                onChange={handleKeyboardChange}
                onReset={() => {
                  handleKeyboardChange(DEFAULT_KEYBOARD_CONFIG);
                  showNotice('键盘键位与轴体参数已恢复默认');
                }}
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
                onChange={handleAudioChange}
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
