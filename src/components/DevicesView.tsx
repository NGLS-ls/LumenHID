import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Battery,
  Bluetooth,
  Download,
  Headphones,
  Keyboard,
  Loader2,
  Mouse,
  Radar,
  Radio,
  RefreshCw,
  Search,
  Upload,
  Usb,
  X,
} from 'lucide-react';
import type {
  BatterySource,
  ConnectionProtocol,
  DeviceCategory,
  NavSection,
  PeripheralDevice,
  ProtocolFilter,
} from '../types/peripheral';
import { filterDevices } from '../utils/deviceEngine';
import { isTauriRuntime, scanRealDevices, supportsWebHid } from '../services/deviceBridge';

interface DevicesViewProps {
  devices: PeripheralDevice[];
  activeDeviceId: string;
  scanning: boolean;
  onSelectDeviceAndOpen: (device: PeripheralDevice, section: NavSection) => void;
  onRefreshDevices: () => Promise<void>;
  onMergeScannedDevices: (found: PeripheralDevice[]) => void;
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
  bluetooth: '蓝牙',
  usb: 'USB 有线',
};

const BATTERY_SOURCE_LABELS: Record<BatterySource, string> = {
  'windows-bt': '蓝牙系统电量',
  'hid-vendor': 'HID 私有协议',
  wired: 'USB 供电',
  unavailable: '硬件未上报电量',
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

function formatBatteryText(dev: PeripheralDevice): string {
  if (dev.batteryPercent !== null) {
    return `电量 ${dev.batteryPercent}% (${BATTERY_SOURCE_LABELS[dev.batterySource]})`;
  }
  if (dev.batterySource === 'wired' || dev.activeProtocol === 'usb') {
    return 'USB 有线供电';
  }
  return '暂未上报电量';
}

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
        alt={`${device.name} 外观示意`}
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
  scanning,
  onSelectDeviceAndOpen,
  onRefreshDevices,
  onMergeScannedDevices,
  onExportProfile,
  onImportProfile,
}: DevicesViewProps) {
  const [protocolFilter, setProtocolFilter] = useState<ProtocolFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [scanOpen, setScanOpen] = useState(false);
  const [modalScanning, setModalScanning] = useState(false);
  const [scanResults, setScanResults] = useState<PeripheralDevice[]>([]);
  const [scanMessage, setScanMessage] = useState<string | null>(null);
  const [importFeedback, setImportFeedback] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const visibleDevices = filterDevices(devices, protocolFilter, searchQuery);

  // Allow closing viewport modal via Escape key
  useEffect(() => {
    if (!scanOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setScanOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [scanOpen]);

  const handleRunRealScan = async () => {
    setModalScanning(true);
    setScanMessage(null);
    const res = await scanRealDevices();
    setScanResults(res.devices);
    setScanMessage(res.message);
    if (res.devices.length > 0) {
      onMergeScannedDevices(res.devices);
    }
    setModalScanning(false);
  };

  const handleOpenScanModal = () => {
    setScanResults([]);
    setScanMessage(null);
    setScanOpen(true);
    if (isTauriRuntime()) {
      void handleRunRealScan();
    }
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

  const maxPolling = devices.length > 0 ? Math.max(...devices.map((d) => d.pollingRateHz)) : 0;

  const scanModal =
    scanOpen && typeof document !== 'undefined'
      ? createPortal(
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="scan-device-title"
            data-testid="scan-device-modal"
            className="fixed inset-0 z-[100] flex items-center justify-center bg-[#181C20]/35 p-4 backdrop-blur-[2px]"
            onClick={(e) => {
              if (e.target === e.currentTarget) setScanOpen(false);
            }}
          >
            <div className="w-full max-w-lg rounded-2xl border border-[#E5E9EE] bg-white p-6 shadow-xl">
              <div className="flex items-center justify-between">
                <div>
                  <h2 id="scan-device-title" className="text-base font-semibold text-[#181C20]">
                    扫描本机真实外设
                  </h2>
                  <p className="mt-0.5 text-xs text-[#586370]">
                    直连枚举 2.4G / 蓝牙 / USB 物理外设与电量，自动过滤 Realtek 声卡与系统内置节点。
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => setScanOpen(false)}
                  aria-label="关闭扫描窗口"
                  className="rounded-lg p-1.5 text-[#586370] hover:bg-[#F0F3F6]"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="mt-4 rounded-xl border border-[#E5E9EE] bg-[#F8FAFB] p-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-xs font-medium text-[#181C20]">
                      {isTauriRuntime() ? 'Windows 原生 HID + 蓝牙 PnP 引擎' : '浏览器 WebHID 硬件端口'}
                    </div>
                    <p className="mt-0.5 text-[11px] leading-relaxed text-[#586370]">
                      {isTauriRuntime()
                        ? '已启用异步非阻塞扫描与 DEVPKEY_Bluetooth_Battery 实时电量读取。'
                        : supportsWebHid()
                          ? '点击右侧按钮向浏览器申请授权 USB / 2.4G 外设（完整蓝牙电量请运行 Windows 桌面版 EXE）。'
                          : '当前浏览器不支持 WebHID，请运行 Windows 便携版 LumenHID.exe 扫描本机外设。'}
                    </p>
                  </div>
                  <button
                    type="button"
                    disabled={modalScanning}
                    onClick={() => void handleRunRealScan()}
                    className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-[#2B6B68] px-3.5 py-2 text-xs font-medium text-white hover:bg-[#215452] disabled:opacity-60 transition-colors whitespace-nowrap"
                  >
                    {modalScanning ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Radar className="h-3.5 w-3.5" />
                    )}
                    <span>{modalScanning ? '扫描中...' : '立即扫描'}</span>
                  </button>
                </div>
              </div>

              {scanMessage && (
                <p
                  role="status"
                  className="mt-3 rounded-xl border border-[#E5E9EE] bg-[#FAFBFD] px-3.5 py-2.5 text-xs leading-relaxed text-[#586370]"
                >
                  {scanMessage}
                </p>
              )}

              {scanResults.length > 0 && (
                <div className="mt-4 max-h-60 space-y-2 overflow-y-auto pr-1">
                  {scanResults.map((dev) => (
                    <div
                      key={dev.id}
                      className="flex items-center justify-between gap-3 rounded-xl border border-[#E5E9EE] bg-white px-3.5 py-2.5"
                    >
                      <div className="min-w-0">
                        <div className="truncate text-xs font-semibold text-[#181C20]">
                          {dev.name}
                        </div>
                        <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px] text-[#586370]">
                          <span>{PROTOCOL_LABELS[dev.activeProtocol]}</span>
                          <span aria-hidden="true">·</span>
                          <span className="font-mono tabular-nums">
                            {dev.vendorId}:{dev.productId}
                          </span>
                          <span aria-hidden="true">·</span>
                          <span className="font-mono tabular-nums text-[#2B6B68]">
                            {formatBatteryText(dev)}
                          </span>
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => {
                          onSelectDeviceAndOpen(dev, CATEGORY_ROUTE[dev.category]);
                          setScanOpen(false);
                        }}
                        className="shrink-0 rounded-lg border border-[#DCE2E8] bg-[#FAFBFD] px-2.5 py-1 text-[11px] font-medium text-[#181C20] hover:border-[#2B6B68] hover:bg-[#EBF3F2] hover:text-[#1D4E4C]"
                      >
                        进入调校
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <div className="mt-5 flex items-center justify-end gap-2 border-t border-[#EEF1F4] pt-4">
                <button
                  type="button"
                  onClick={() => setScanOpen(false)}
                  className="rounded-lg bg-[#181C20] px-4 py-2 text-xs font-medium text-white hover:bg-[#2C333B]"
                >
                  完成
                </button>
              </div>
            </div>
          </div>,
          document.body
        )
      : null;

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#181C20] [text-wrap:balance]">
            多协议设备总览
          </h1>
          <p className="mt-1 text-sm text-[#586370]">
            实时纳管本机 2.4G、蓝牙与 USB 外设并读取电量，自动屏蔽主板声卡与系统虚拟端点。
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
            disabled={scanning}
            onClick={() => void onRefreshDevices()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] disabled:opacity-60 transition-colors whitespace-nowrap shrink-0"
          >
            <RefreshCw className={`h-3.5 w-3.5 text-[#586370] ${scanning ? 'animate-spin' : ''}`} />
            <span>{scanning ? '刷新中...' : '刷新设备与电量'}</span>
          </button>
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] transition-colors whitespace-nowrap shrink-0"
          >
            <Upload className="h-3.5 w-3.5 text-[#586370]" />
            <span>导入配置</span>
          </button>
          <button
            type="button"
            onClick={onExportProfile}
            className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#181C20] hover:bg-[#F0F3F6] transition-colors whitespace-nowrap shrink-0"
          >
            <Download className="h-3.5 w-3.5 text-[#586370]" />
            <span>导出 JSON</span>
          </button>
          <button
            type="button"
            onClick={handleOpenScanModal}
            className="inline-flex items-center gap-1.5 rounded-lg bg-[#2B6B68] px-4 py-2 text-xs font-medium text-white hover:bg-[#215452] transition-colors whitespace-nowrap shrink-0"
          >
            <Radar className="h-3.5 w-3.5" />
            <span>扫描本机外设</span>
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
            placeholder="搜索名称、厂商或 VID/PID..."
            aria-label="搜索外设"
            className="w-full rounded-xl border border-[#E5E9EE] bg-white py-2 pl-9 pr-3.5 text-xs text-[#181C20] placeholder:text-[#84909D] focus:border-[#2B6B68] focus:outline-none"
          />
        </div>
      </div>

      {/* Summary Metrics Strip */}
      <div className="grid grid-cols-2 gap-4 rounded-2xl border border-[#E5E9EE] bg-white px-6 py-4 sm:grid-cols-4">
        <div>
          <div className="text-xs text-[#84909D]">本机物理外设</div>
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
          <div className="text-xs text-[#84909D]">已识别电量设备</div>
          <div className="mt-1 font-mono text-xl font-semibold tabular-nums text-[#181C20]">
            {devices.filter((d) => d.batteryPercent !== null).length}
            <span className="ml-1 text-xs font-normal text-[#84909D]">台</span>
          </div>
        </div>
        <div>
          <div className="text-xs text-[#84909D]">最高回报率</div>
          <div className="mt-1 font-mono text-xl font-semibold tabular-nums text-[#181C20]">
            {maxPolling > 0 ? maxPolling : '--'}
            {maxPolling > 0 && <span className="ml-1 text-xs font-normal text-[#84909D]">Hz</span>}
          </div>
        </div>
      </div>

      {/* Device Grid or Clean Real-Hardware Empty State */}
      {devices.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-[#E5E9EE] bg-white px-6 py-16 text-center">
          <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-[#EBF3F2] text-[#2B6B68]">
            <Radar className="h-6 w-6" strokeWidth={1.6} />
          </div>
          <h2 className="mt-4 text-base font-semibold text-[#181C20]">
            当前未检测到已接入的物理外设
          </h2>
          <p className="mt-1.5 max-w-md text-xs leading-relaxed text-[#586370]">
            已完全移除模拟示例数据。请插入 2.4G 接收器、USB 线缆或配对蓝牙设备后点击下方扫描，程序会自动过滤 Realtek 等主板内置音频与虚拟节点。
          </p>
          <button
            type="button"
            onClick={handleOpenScanModal}
            className="mt-5 inline-flex items-center gap-2 rounded-lg bg-[#2B6B68] px-4 py-2.5 text-xs font-medium text-white hover:bg-[#215452] transition-colors"
          >
            <Radar className="h-3.5 w-3.5" />
            <span>立即扫描本机外设</span>
          </button>
        </div>
      ) : visibleDevices.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-[#E5E9EE] bg-white px-6 py-14 text-center">
          <p className="text-sm font-medium text-[#181C20]">没有符合当前筛选条件的外设</p>
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
            const ProtoIcon =
              dev.activeProtocol === '2.4g'
                ? Radio
                : dev.activeProtocol === 'bluetooth'
                  ? Bluetooth
                  : Usb;
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
                    <span className="inline-flex shrink-0 items-center gap-1 rounded-lg bg-[#EBF3F2] px-2.5 py-1 text-xs font-medium text-[#1D4E4C] whitespace-nowrap">
                      <ProtoIcon className="h-3 w-3" />
                      <span>{PROTOCOL_LABELS[dev.activeProtocol]}</span>
                    </span>
                  </div>

                  {/* Clean unboxed telemetry row */}
                  <div className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-[#586370]">
                    <span className="inline-flex items-center gap-1 font-mono font-medium tabular-nums text-[#1D4E4C]">
                      <Battery className="h-3.5 w-3.5 text-[#2B6B68]" />
                      {formatBatteryText(dev)}
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
                  </div>

                  {/* Honest protocol support explanation */}
                  <p className="mt-3 border-t border-[#EEF1F4] pt-3 text-[11px] leading-relaxed text-[#586370]">
                    {dev.protocolNote}
                  </p>

                  <div className="mt-auto pt-4">
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

      {scanModal}
    </div>
  );
}
