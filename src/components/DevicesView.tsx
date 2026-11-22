import { useRef, useState } from 'react';
import {
  Bluetooth,
  Download,
  Headphones,
  Keyboard,
  Mouse,
  Plus,
  Radio,
  Search,
  Upload,
  Usb,
  X,
} from 'lucide-react';
import type {
  ConnectionProtocol,
  DeviceCategory,
  NavSection,
  PeripheralDevice,
  ProtocolFilter,
} from '../types/peripheral';
import { filterDevices } from '../utils/deviceEngine';
import { requestWebHidDevice, supportsWebHid } from '../services/deviceBridge';

interface DevicesViewProps {
  devices: PeripheralDevice[];
  activeDeviceId: string;
  onSelectDeviceAndOpen: (device: PeripheralDevice, section: NavSection) => void;
  onSwitchProtocol: (deviceId: string, protocol: ConnectionProtocol) => void;
  onToggleConnect: (deviceId: string) => void;
  onChangeOnboardSlot: (deviceId: string, slot: number) => void;
  onAddDevice: (device: PeripheralDevice) => void;
  onExportProfile: () => void;
  onImportProfile: (rawJson: string) => { ok: boolean; message: string };
}

const PROTOCOL_FILTERS: { id: ProtocolFilter; label: string }[] = [
  { id: 'all', label: '全部协议' },
  { id: '2.4g', label: '2.4G 无线' },
  { id: 'bluetooth', label: '蓝牙' },
  { id: 'usb', label: 'USB 有线' },
];

const PROTOCOL_LABELS: Record<ConnectionProtocol, string> = {
  '2.4g': '2.4G 无线',
  bluetooth: '蓝牙 5.3',
  usb: 'USB-C 有线',
};

const CATEGORY_ROUTE: Record<DeviceCategory, NavSection> = {
  mouse: 'mouse',
  keyboard: 'keyboard',
  headset: 'audio',
};

const CATEGORY_ACTION_LABEL: Record<DeviceCategory, string> = {
  mouse: '进入鼠标调校',
  keyboard: '进入键轴调校',
  headset: '进入声学调校',
};

function DeviceThumbnail({ device }: { device: PeripheralDevice }) {
  const [failed, setFailed] = useState(false);
  const Icon =
    device.category === 'mouse'
      ? Mouse
      : device.category === 'keyboard'
        ? Keyboard
        : Headphones;

  if (failed || !device.imageUrl) {
    return (
      <div className="flex aspect-[4/3] w-full flex-col items-center justify-center bg-[#F0F3F6] text-[#84909D]">
        <Icon className="h-9 w-9" strokeWidth={1.4} />
        <span className="mt-2 text-xs font-medium text-[#586370]">{device.name}</span>
      </div>
    );
  }

  return (
    <div className="relative aspect-[4/3] w-full overflow-hidden bg-[#EEF1F4]">
      <img
        src={device.imageUrl}
        alt={`${device.name} 外观渲染图`}
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        className="h-full w-full object-cover transition-transform duration-300 ease-out group-hover:scale-[1.02]"
      />
    </div>
  );
}

export function DevicesView({
  devices,
  activeDeviceId,
  onSelectDeviceAndOpen,
  onSwitchProtocol,
  onToggleConnect,
  onChangeOnboardSlot,
  onAddDevice,
  onExportProfile,
  onImportProfile,
}: DevicesViewProps) {
  const [protocolFilter, setProtocolFilter] = useState<ProtocolFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [scanOpen, setScanOpen] = useState(false);
  const [newName, setNewName] = useState('');
  const [newCategory, setNewCategory] = useState<DeviceCategory>('mouse');
  const [newProtocol, setNewProtocol] = useState<ConnectionProtocol>('2.4g');
  const [formError, setFormError] = useState<string | null>(null);
  const [importFeedback, setImportFeedback] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const visibleDevices = filterDevices(devices, protocolFilter, searchQuery);

  const handleWebHidPair = async () => {
    const discovered = await requestWebHidDevice();
    if (discovered) {
      onAddDevice(discovered);
      setScanOpen(false);
    } else {
      setFormError('未检测到可用的 WebHID 设备或已取消授权，可在下方手动添加离线配置文件。');
    }
  };

  const handleManualAdd = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = newName.trim();
    if (!trimmed) {
      setFormError('请输入外设名称');
      return;
    }
    const imageMap: Record<DeviceCategory, string> = {
      mouse: '/src/assets/images/device_mouse_minimal_1795334434447.jpg',
      keyboard: '/src/assets/images/device_keyboard_minimal_1795334434542.jpg',
      headset: '/src/assets/images/device_headset_minimal_1795334435205.jpg',
    };
    const modelMap: Record<DeviceCategory, string> = {
      mouse: '通用多模无线鼠标',
      keyboard: '通用三模机械/磁轴键盘',
      headset: '通用无线声学耳机',
    };
    const hexPid = `0x${Math.floor(0x1000 + Math.random() * 0xefff)
      .toString(16)
      .toUpperCase()}`;

    onAddDevice({
      id: `dev-custom-${Date.now()}`,
      name: trimmed,
      model: modelMap[newCategory],
      category: newCategory,
      activeProtocol: newProtocol,
      supportedProtocols: [newProtocol, 'usb'],
      connected: true,
      batteryPercent: newProtocol === 'usb' ? null : 92,
      charging: newProtocol === 'usb',
      firmwareVersion: '1.0.0',
      vendorId: '0x3554',
      productId: hexPid,
      pollingRateHz: newCategory === 'headset' ? 1000 : 2000,
      signalStrengthDbm: newProtocol === 'usb' ? null : -46,
      imageUrl: imageMap[newCategory],
      onboardSlots: 3,
      activeSlot: 1,
      sleepMinutes: 10,
    });
    setNewName('');
    setFormError(null);
    setScanOpen(false);
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const result = onImportProfile(String(reader.result ?? ''));
      setImportFeedback(result.message);
    };
    reader.readAsText(file);
    e.target.value = '';
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20] [text-wrap:balance]">
            多协议设备总览
          </h1>
          <p className="mt-1 text-sm text-[#586370]">
            统一纳管 2.4G 无线接收器、蓝牙与 USB 有线外设，配置自动写入本地便携目录。
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <input
            ref={fileInputRef}
            type="file"
            accept="application/json,.json"
            onChange={handleFileChange}
            className="hidden"
            aria-label="导入便携配置文件"
          />
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            className="inline-flex items-center gap-2 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] transition-colors whitespace-nowrap shrink-0"
          >
            <Upload className="h-3.5 w-3.5 text-[#586370]" />
            <span>导入配置</span>
          </button>
          <button
            type="button"
            onClick={onExportProfile}
            className="inline-flex items-center gap-2 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] transition-colors whitespace-nowrap shrink-0"
          >
            <Download className="h-3.5 w-3.5 text-[#586370]" />
            <span>导出便携 JSON</span>
          </button>
          <button
            type="button"
            onClick={() => {
              setFormError(null);
              setScanOpen(true);
            }}
            className="inline-flex items-center gap-2 rounded-lg bg-[#2B6B68] px-4 py-2 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap shrink-0"
          >
            <Plus className="h-3.5 w-3.5" />
            <span>接入新设备</span>
          </button>
        </div>
      </div>

      {importFeedback && (
        <div
          role="status"
          className="flex items-center justify-between rounded-xl border border-[#C4DCD9] bg-[#EBF3F2] px-4 py-2.5 text-xs text-[#1D4E4C]"
        >
          <span>{importFeedback}</span>
          <button
            type="button"
            onClick={() => setImportFeedback(null)}
            aria-label="关闭导入提示"
            className="text-[#1D4E4C] hover:opacity-75"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      {/* Filter & Search Bar */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div
          role="tablist"
          aria-label="按连接协议筛选设备"
          className="inline-flex items-center gap-1 rounded-xl bg-[#EEF1F4] p-1 self-start"
        >
          {PROTOCOL_FILTERS.map((tab) => {
            const active = protocolFilter === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setProtocolFilter(tab.id)}
                className={`rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all duration-150 whitespace-nowrap shrink-0 ${
                  active
                    ? 'bg-white text-[#181C20] shadow-xs'
                    : 'text-[#586370] hover:text-[#181C20]'
                }`}
              >
                {tab.label}
              </button>
            );
          })}
        </div>

        <div className="relative w-full sm:w-72">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[#84909D]"
            strokeWidth={1.75}
          />
          <input
            type="search"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="搜索名称、型号或 VID/PID..."
            aria-label="搜索外设"
            className="w-full rounded-xl border border-[#E5E9EE] bg-white py-2 pl-9 pr-3.5 text-xs text-[#181C20] placeholder:text-[#84909D] focus:border-[#2B6B68] focus:outline-none"
          />
        </div>
      </div>

      {/* Summary Metrics Strip (Flat hairline row, no nested cards) */}
      <div className="grid grid-cols-2 gap-4 rounded-2xl border border-[#E5E9EE] bg-white px-6 py-4 sm:grid-cols-4">
        <div>
          <div className="text-xs text-[#84909D]">在线外设</div>
          <div className="mt-1 font-mono text-xl font-semibold tabular-nums text-[#181C20]">
            {devices.filter((d) => d.connected).length}
            <span className="text-sm font-normal text-[#84909D]"> / {devices.length}</span>
          </div>
        </div>
        <div>
          <div className="text-xs text-[#84909D]">2.4G / 蓝牙 / USB</div>
          <div className="mt-1 font-mono text-xl font-semibold tabular-nums text-[#181C20]">
            {devices.filter((d) => d.activeProtocol === '2.4g').length}
            <span className="mx-1 text-[#CBD3DC]">/</span>
            {devices.filter((d) => d.activeProtocol === 'bluetooth').length}
            <span className="mx-1 text-[#CBD3DC]">/</span>
            {devices.filter((d) => d.activeProtocol === 'usb').length}
          </div>
        </div>
        <div>
          <div className="text-xs text-[#84909D]">最高回报率</div>
          <div className="mt-1 font-mono text-xl font-semibold tabular-nums text-[#181C20]">
            {Math.max(...devices.map((d) => d.pollingRateHz), 1000)}
            <span className="ml-1 text-xs font-normal text-[#84909D]">Hz</span>
          </div>
        </div>
        <div>
          <div className="text-xs text-[#84909D]">运行存储模式</div>
          <div className="mt-1 text-sm font-medium text-[#1D4E4C] pt-1">
            免安装本地便携
          </div>
        </div>
      </div>

      {/* Device Grid */}
      {visibleDevices.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-[#E5E9EE] bg-white px-6 py-16 text-center">
          <p className="text-sm font-medium text-[#181C20]">未找到匹配的外设</p>
          <p className="mt-1 text-xs text-[#586370]">
            尝试切换连接协议或清空搜索关键字，也可以手动接入新的外设配置。
          </p>
          <button
            type="button"
            onClick={() => {
              setProtocolFilter('all');
              setSearchQuery('');
            }}
            className="mt-4 rounded-lg border border-[#E5E9EE] bg-[#F6F7F9] px-4 py-2 text-xs font-medium text-[#181C20] hover:bg-[#EBF3F2] transition-colors"
          >
            重置筛选条件
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3">
          {visibleDevices.map((dev) => {
            const isSelected = dev.id === activeDeviceId;
            return (
              <article
                key={dev.id}
                className={`group flex flex-col overflow-hidden rounded-2xl border bg-white transition-colors ${
                  isSelected ? 'border-[#2B6B68]' : 'border-[#E5E9EE] hover:border-[#C9D3DC]'
                }`}
              >
                <DeviceThumbnail device={dev} />

                <div className="flex flex-1 flex-col p-5">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <h2 className="truncate text-base font-semibold text-[#181C20]">
                        {dev.name}
                      </h2>
                      <p className="mt-0.5 truncate text-xs text-[#586370]">{dev.model}</p>
                    </div>
                    <button
                      type="button"
                      onClick={() => onToggleConnect(dev.id)}
                      aria-label={`切换 ${dev.name} 连接状态`}
                      className={`shrink-0 rounded-lg px-2.5 py-1 text-xs font-medium transition-colors whitespace-nowrap ${
                        dev.connected
                          ? 'bg-[#EBF3F2] text-[#1D4E4C] hover:bg-[#DCEBE9]'
                          : 'bg-[#F0F3F6] text-[#586370] hover:bg-[#E5E9EE]'
                      }`}
                    >
                      {dev.connected ? '已连接' : '已休眠'}
                    </button>
                  </div>

                  {/* Clean unboxed metadata row */}
                  <div className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-[#586370]">
                    <span>{PROTOCOL_LABELS[dev.activeProtocol]}</span>
                    <span aria-hidden="true" className="text-[#CBD3DC]">
                      ·
                    </span>
                    <span className="font-mono tabular-nums">
                      {dev.batteryPercent !== null
                        ? `电量 ${dev.batteryPercent}%${dev.charging ? ' (充电中)' : ''}`
                        : 'USB 直连供电'}
                    </span>
                    <span aria-hidden="true" className="text-[#CBD3DC]">
                      ·
                    </span>
                    <span className="font-mono tabular-nums">{dev.pollingRateHz} Hz</span>
                  </div>

                  <div className="mt-1.5 flex items-center gap-2 font-mono text-[11px] tabular-nums text-[#84909D]">
                    <span>VID {dev.vendorId}</span>
                    <span aria-hidden="true">/</span>
                    <span>PID {dev.productId}</span>
                    <span aria-hidden="true">/</span>
                    <span>FW {dev.firmwareVersion}</span>
                  </div>

                  {/* Protocol Quick Switcher */}
                  <div className="mt-4 border-t border-[#EEF1F4] pt-4">
                    <div className="flex items-center justify-between text-xs text-[#84909D]">
                      <span>通道切换</span>
                      <span>板载配置槽位</span>
                    </div>
                    <div className="mt-2 flex items-center justify-between gap-2">
                      <div className="inline-flex items-center gap-1 rounded-lg bg-[#F4F6F8] p-0.5">
                        {dev.supportedProtocols.map((proto) => {
                          const Icon =
                            proto === '2.4g' ? Radio : proto === 'bluetooth' ? Bluetooth : Usb;
                          const active = dev.activeProtocol === proto;
                          return (
                            <button
                              key={proto}
                              type="button"
                              onClick={() => onSwitchProtocol(dev.id, proto)}
                              aria-label={`将 ${dev.name} 切换至 ${PROTOCOL_LABELS[proto]}`}
                              className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors whitespace-nowrap ${
                                active
                                  ? 'bg-white text-[#181C20] shadow-xs'
                                  : 'text-[#586370] hover:text-[#181C20]'
                              }`}
                            >
                              <Icon className="h-3 w-3" />
                              <span>{proto === '2.4g' ? '2.4G' : proto === 'bluetooth' ? '蓝牙' : 'USB'}</span>
                            </button>
                          );
                        })}
                      </div>

                      <div className="inline-flex items-center gap-1">
                        {Array.from({ length: dev.onboardSlots }, (_, i) => i + 1).map((slot) => (
                          <button
                            key={slot}
                            type="button"
                            onClick={() => onChangeOnboardSlot(dev.id, slot)}
                            aria-label={`${dev.name} 板载槽位 ${slot}`}
                            className={`h-6 w-6 rounded-md font-mono text-[11px] font-medium tabular-nums transition-colors ${
                              dev.activeSlot === slot
                                ? 'bg-[#2B6B68] text-white'
                                : 'bg-[#F4F6F8] text-[#586370] hover:bg-[#E5E9EE]'
                            }`}
                          >
                            {slot}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>

                  <div className="mt-5 pt-1">
                    <button
                      type="button"
                      onClick={() => onSelectDeviceAndOpen(dev, CATEGORY_ROUTE[dev.category])}
                      className="w-full rounded-xl border border-[#DCE2E8] bg-[#FAFBFD] py-2.5 text-xs font-medium text-[#181C20] hover:border-[#2B6B68] hover:bg-[#EBF3F2] hover:text-[#1D4E4C] transition-colors"
                    >
                      {CATEGORY_ACTION_LABEL[dev.category]}
                    </button>
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      )}

      {/* Add / Scan Device Modal */}
      {scanOpen && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="scan-device-title"
          className="fixed inset-0 z-50 flex items-center justify-center bg-[#181C20]/30 p-4 backdrop-blur-[2px]"
        >
          <div className="w-full max-w-md rounded-2xl border border-[#E5E9EE] bg-white p-6 shadow-xl">
            <div className="flex items-center justify-between">
              <h2 id="scan-device-title" className="text-base font-semibold text-[#181C20]">
                扫描或添加外设
              </h2>
              <button
                type="button"
                onClick={() => setScanOpen(false)}
                aria-label="关闭接入窗口"
                className="rounded-lg p-1.5 text-[#586370] hover:bg-[#F0F3F6]"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 rounded-xl border border-[#E5E9EE] bg-[#F8FAFB] p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-xs font-medium text-[#181C20]">硬件直连扫描</div>
                  <p className="mt-0.5 text-[11px] text-[#586370]">
                    {supportsWebHid()
                      ? '当前环境支持 WebHID，可直接向浏览器申请读取 USB / 2.4G 接收器。'
                      : '桌面版 EXE 通过 Rust hidapi 自动扫描 2.4G、蓝牙与 USB 设备。'}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={handleWebHidPair}
                  className="shrink-0 rounded-lg bg-[#2B6B68] px-3 py-1.5 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap"
                >
                  开始扫描
                </button>
              </div>
            </div>

            <form onSubmit={handleManualAdd} className="mt-5 space-y-4">
              <div className="text-xs font-medium text-[#586370]">或新建离线外设配置档案</div>

              <div>
                <label htmlFor="new-device-name" className="block text-xs font-medium text-[#181C20]">
                  设备名称
                </label>
                <input
                  id="new-device-name"
                  type="text"
                  value={newName}
                  onChange={(e) => {
                    setNewName(e.target.value);
                    setFormError(null);
                  }}
                  placeholder="例如：Lumen Mini 8K / 蓝牙办公鼠标"
                  className="mt-1.5 w-full rounded-xl border border-[#E5E9EE] px-3.5 py-2 text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label htmlFor="new-device-category" className="block text-xs font-medium text-[#181C20]">
                    设备类型
                  </label>
                  <select
                    id="new-device-category"
                    value={newCategory}
                    onChange={(e) => setNewCategory(e.target.value as DeviceCategory)}
                    className="mt-1.5 w-full rounded-xl border border-[#E5E9EE] bg-white px-3 py-2 text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                  >
                    <option value="mouse">鼠标</option>
                    <option value="keyboard">键盘</option>
                    <option value="headset">耳机</option>
                  </select>
                </div>

                <div>
                  <label htmlFor="new-device-protocol" className="block text-xs font-medium text-[#181C20]">
                    默认协议
                  </label>
                  <select
                    id="new-device-protocol"
                    value={newProtocol}
                    onChange={(e) => setNewProtocol(e.target.value as ConnectionProtocol)}
                    className="mt-1.5 w-full rounded-xl border border-[#E5E9EE] bg-white px-3 py-2 text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                  >
                    <option value="2.4g">2.4G 无线</option>
                    <option value="bluetooth">蓝牙</option>
                    <option value="usb">USB 有线</option>
                  </select>
                </div>
              </div>

              {formError && (
                <p role="alert" className="text-xs text-[#B91C1C]">
                  {formError}
                </p>
              )}

              <div className="flex items-center justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setScanOpen(false)}
                  className="rounded-lg border border-[#E5E9EE] px-4 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6]"
                >
                  取消
                </button>
                <button
                  type="submit"
                  className="rounded-lg bg-[#181C20] px-4 py-2 text-xs font-medium text-white hover:bg-[#2C333B]"
                >
                  添加外设
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
