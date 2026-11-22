import { useEffect, useState } from 'react';
import { Menu, Save } from 'lucide-react';
import { Sidebar } from './components/Sidebar';
import { DevicesView } from './components/DevicesView';
import { MouseView } from './components/MouseView';
import { KeyboardView } from './components/KeyboardView';
import { MacroStudioView } from './components/MacroStudioView';
import { AudioStudioView } from './components/AudioStudioView';
import { DocsHubView } from './components/DocsHubView';
import {
  INITIAL_AUDIO_CONFIG,
  INITIAL_DEVICES,
  INITIAL_KEYBOARD_CONFIG,
  INITIAL_MACROS,
  INITIAL_MOUSE_CONFIG,
} from './data/seedDevices';
import type {
  AppProfileSnapshot,
  AudioConfig,
  ConnectionProtocol,
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
  docs: '工程与文档',
};

export default function App() {
  const initialSnapshot = loadSavedProfile();

  const [activeSection, setActiveSection] = useState<NavSection>('devices');
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  const [devices, setDevices] = useState<PeripheralDevice[]>(
    initialSnapshot?.devices ?? INITIAL_DEVICES
  );
  const [activeDeviceId, setActiveDeviceId] = useState<string>(
    initialSnapshot?.activeDeviceId ?? INITIAL_DEVICES[0].id
  );
  const [mouseConfig, setMouseConfig] = useState<MouseConfig>(
    initialSnapshot?.mouseConfig ?? INITIAL_MOUSE_CONFIG
  );
  const [keyboardConfig, setKeyboardConfig] = useState<KeyboardConfig>(
    initialSnapshot?.keyboardConfig ?? INITIAL_KEYBOARD_CONFIG
  );
  const [macros, setMacros] = useState<MacroDefinition[]>(
    initialSnapshot?.macros ?? INITIAL_MACROS
  );
  const [audioConfig, setAudioConfig] = useState<AudioConfig>(
    initialSnapshot?.audioConfig ?? INITIAL_AUDIO_CONFIG
  );

  const [notice, setNotice] = useState<string | null>(null);

  const showNotice = (msg: string) => {
    setNotice(msg);
  };

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 2600);
    return () => window.clearTimeout(timer);
  }, [notice]);

  // Auto-discover real Windows native HID ports when running inside portable Tauri EXE
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void scanNativeWindowsDevices().then((nativeDevs) => {
      if (cancelled || nativeDevs.length === 0) return;
      setDevices((prev) => {
        const existingKeys = new Set(prev.map((d) => `${d.vendorId}:${d.productId}`));
        const fresh = nativeDevs.filter((d) => !existingKeys.has(`${d.vendorId}:${d.productId}`));
        return fresh.length > 0 ? [...fresh, ...prev] : prev;
      });
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Auto-persist portable snapshot cleanly whenever state changes
  useEffect(() => {
    const snapshot: AppProfileSnapshot = {
      schemaVersion: '1.0.0',
      exportedAt: new Date().toISOString(),
      activeDeviceId,
      devices,
      mouseConfig,
      keyboardConfig,
      macros,
      audioConfig,
    };
    void saveProfileSnapshot(snapshot);
  }, [activeDeviceId, devices, mouseConfig, keyboardConfig, macros, audioConfig]);

  const mouseDevice =
    devices.find((d) => d.category === 'mouse' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'mouse') ??
    INITIAL_DEVICES[0];

  const keyboardDevice =
    devices.find((d) => d.category === 'keyboard' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'keyboard') ??
    INITIAL_DEVICES[1];

  const headsetDevice =
    devices.find((d) => d.category === 'headset' && d.id === activeDeviceId) ??
    devices.find((d) => d.category === 'headset') ??
    INITIAL_DEVICES[2];

  const handleSelectDevice = (device: PeripheralDevice) => {
    setActiveDeviceId(device.id);
    if (device.category === 'mouse') setActiveSection('mouse');
    else if (device.category === 'keyboard') setActiveSection('keyboard');
    else setActiveSection('audio');
  };

  const handleMouseChange = (next: MouseConfig) => {
    setMouseConfig(next);
    setDevices((prev) =>
      prev.map((d) =>
        d.id === mouseDevice.id ? { ...d, pollingRateHz: next.pollingRateHz } : d
      )
    );
    void syncMouseHardware(next, mouseDevice);
  };

  const handleKeyboardChange = (next: KeyboardConfig) => {
    setKeyboardConfig(next);
    setDevices((prev) =>
      prev.map((d) =>
        d.id === keyboardDevice.id ? { ...d, pollingRateHz: next.pollingRateHz } : d
      )
    );
    void syncKeyboardHardware(next, keyboardDevice);
  };

  const handleAudioChange = (next: AudioConfig) => {
    setAudioConfig(next);
    void syncAudioHardware(next, headsetDevice);
  };

  const handleSwitchProtocol = (deviceId: string, protocol: ConnectionProtocol) => {
    setDevices((prev) =>
      prev.map((d) =>
        d.id === deviceId
          ? {
              ...d,
              activeProtocol: protocol,
              connected: true,
              charging: protocol === 'usb',
              signalStrengthDbm: protocol === 'usb' ? null : -45,
            }
          : d
      )
    );
    showNotice(`已切换通道至 ${protocol === '2.4g' ? '2.4G 无线' : protocol === 'bluetooth' ? '蓝牙' : 'USB 有线'}`);
  };

  const handleToggleConnect = (deviceId: string) => {
    setDevices((prev) =>
      prev.map((d) => (d.id === deviceId ? { ...d, connected: !d.connected } : d))
    );
  };

  const handleChangeOnboardSlot = (deviceId: string, slot: number) => {
    setDevices((prev) =>
      prev.map((d) => (d.id === deviceId ? { ...d, activeSlot: slot } : d))
    );
    showNotice(`已切换至板载存储槽位 ${slot}`);
  };

  const handleAddDevice = (device: PeripheralDevice) => {
    setDevices((prev) => [device, ...prev]);
    setActiveDeviceId(device.id);
    showNotice(`已接入外设：${device.name}`);
  };

  const handleExportProfile = () => {
    const snapshot: AppProfileSnapshot = {
      schemaVersion: '1.0.0',
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
    void syncMouseHardware(mouseConfig, mouseDevice);
    void syncKeyboardHardware(keyboardConfig, keyboardDevice);
    void syncAudioHardware(audioConfig, headsetDevice);
    showNotice('已将当前全部配置写入板载内存与便携目录');
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
        {/* Clean Minimalist Top Workspace Bar */}
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
            <span>写入板载配置</span>
          </button>
        </header>

        {/* Main Viewport with Smooth Compositor-Only Transitions */}
        <main className="mx-auto w-full max-w-[1220px] flex-1 px-4 py-6 sm:px-8 sm:py-8">
          <div key={activeSection} className="lumen-view-enter">
              {activeSection === 'devices' && (
                <DevicesView
                  devices={devices}
                  activeDeviceId={activeDeviceId}
                  onSelectDeviceAndOpen={(dev, sec) => {
                    setActiveDeviceId(dev.id);
                    setActiveSection(sec);
                  }}
                  onSwitchProtocol={handleSwitchProtocol}
                  onToggleConnect={handleToggleConnect}
                  onChangeOnboardSlot={handleChangeOnboardSlot}
                  onAddDevice={handleAddDevice}
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
                    handleMouseChange(INITIAL_MOUSE_CONFIG);
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
                    handleKeyboardChange(INITIAL_KEYBOARD_CONFIG);
                    showNotice('键盘键位与磁轴参数已恢复默认');
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
                      ...INITIAL_AUDIO_CONFIG,
                      activePresetId: 'flat',
                      bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                      preampDb: 0,
                    });
                    showNotice('均衡器曲线已重置为纯净原声');
                  }}
                />
              )}

              {activeSection === 'docs' && <DocsHubView />}
          </div>
        </main>
      </div>

      {/* Subtle Accessible Toast Feedback */}
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
