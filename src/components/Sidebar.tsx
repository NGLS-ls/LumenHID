import {
  ArrowUpCircle,
  Cpu,
  Headphones,
  Keyboard,
  Mouse,
  Workflow,
  X,
} from 'lucide-react';
import type { NavSection, PeripheralDevice } from '../types/peripheral';
import type { UpdateCheckResult } from '../services/deviceBridge';

interface SidebarProps {
  activeSection: NavSection;
  onSelectSection: (section: NavSection) => void;
  devices: PeripheralDevice[];
  activeDeviceId: string;
  onSelectDevice: (device: PeripheralDevice) => void;
  mobileOpen: boolean;
  onCloseMobile: () => void;
  appVersion?: string;
  updateInfo?: UpdateCheckResult | null;
  checkingUpdate?: boolean;
  onOpenUpdateModal?: () => void;
}

const NAV_ITEMS: { id: NavSection; label: string; icon: typeof Cpu }[] = [
  { id: 'devices', label: '设备总览', icon: Cpu },
  { id: 'mouse', label: '鼠标调校', icon: Mouse },
  { id: 'keyboard', label: '键盘与轴体', icon: Keyboard },
  { id: 'macros', label: '宏工作台', icon: Workflow },
  { id: 'audio', label: '音频与声场', icon: Headphones },
];

const PROTOCOL_TEXT: Record<PeripheralDevice['activeProtocol'], string> = {
  '2.4g': '2.4G',
  bluetooth: '蓝牙',
  usb: 'USB',
};

function formatCompactBattery(dev: PeripheralDevice): string {
  if (dev.batteryPercent !== null) return `${dev.batteryPercent}%`;
  if (dev.batterySource === 'wired' || dev.activeProtocol === 'usb') return '有线';
  return '--';
}

export function Sidebar({
  activeSection,
  onSelectSection,
  devices,
  activeDeviceId,
  onSelectDevice,
  mobileOpen,
  onCloseMobile,
  appVersion,
  updateInfo,
  checkingUpdate,
  onOpenUpdateModal,
}: SidebarProps) {
  const navContent = (
    <div className="flex h-full flex-col bg-white border-r border-[#E5E9EE]">
      <div className="flex h-16 items-center justify-between px-5 border-b border-[#E5E9EE]">
        <div className="flex items-center gap-2.5 min-w-0">
          <img
            src="/lumen-icon.svg"
            alt="LumenHID 标志"
            className="h-7 w-7 rounded-lg shrink-0"
          />
          <span className="text-[17px] font-semibold tracking-tight text-[#181C20] whitespace-nowrap shrink-0">
            LumenHID
          </span>
        </div>
        <button
          type="button"
          onClick={onCloseMobile}
          aria-label="关闭侧边导航"
          className="lg:hidden rounded-lg p-2 text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] transition-colors"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <nav aria-label="主功能导航" className="flex-1 overflow-y-auto px-3 py-5">
        <ul className="space-y-1">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activeSection === item.id;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => {
                    onSelectSection(item.id);
                    onCloseMobile();
                  }}
                  aria-current={isActive ? 'page' : undefined}
                  className={`relative flex w-full items-center gap-3 rounded-lg px-3.5 py-2.5 text-sm font-medium transition-all duration-150 whitespace-nowrap ${
                    isActive
                      ? 'bg-[#EBF3F2] text-[#1D4E4C]'
                      : 'text-[#586370] hover:text-[#181C20] hover:bg-[#F6F7F9]'
                  }`}
                >
                  <Icon className="h-4 w-4 shrink-0" strokeWidth={1.75} />
                  <span>{item.label}</span>
                </button>
              </li>
            );
          })}
        </ul>

        <div className="mt-8 border-t border-[#E5E9EE] pt-5 px-2">
          <div className="mb-3 flex items-center justify-between text-xs font-medium text-[#84909D]">
            <span>本机已识别外设</span>
            <span className="font-mono tabular-nums">
              {devices.filter((d) => d.connected).length}/{devices.length}
            </span>
          </div>

          {devices.length === 0 ? (
            <p className="px-1.5 text-xs leading-relaxed text-[#84909D]">
              暂未识别到外接键鼠或蓝牙耳机，请点击「扫描本机外设」。
            </p>
          ) : (
            <ul className="space-y-1.5">
              {devices.map((dev) => {
                const selected = dev.id === activeDeviceId;
                return (
                  <li key={dev.id}>
                    <button
                      type="button"
                      onClick={() => {
                        onSelectDevice(dev);
                        onCloseMobile();
                      }}
                      className={`w-full rounded-lg px-2.5 py-2 text-left transition-colors ${
                        selected
                          ? 'bg-[#F0F3F6] text-[#181C20]'
                          : 'text-[#586370] hover:bg-[#F6F7F9] hover:text-[#181C20]'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-[13px] font-medium">{dev.name}</span>
                        <span className="font-mono text-[11px] tabular-nums text-[#2B6B68] shrink-0">
                          {formatCompactBattery(dev)}
                        </span>
                      </div>
                      <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-[#84909D]">
                        <span>{PROTOCOL_TEXT[dev.activeProtocol]}</span>
                        <span aria-hidden="true">·</span>
                        <span>{dev.connected ? '已连接' : '已断开'}</span>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </nav>

      <div className="border-t border-[#E5E9EE] px-5 py-3.5 text-xs text-[#84909D]">
        <div className="flex items-center justify-between gap-2">
          <span className="font-mono tabular-nums text-[#586370]">
            v{appVersion ?? '1.0.0'}
          </span>
          {onOpenUpdateModal ? (
            <button
              type="button"
              onClick={onOpenUpdateModal}
              className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors ${
                updateInfo?.has_update
                  ? 'bg-[#EBF3F2] text-[#1D4E4C] hover:bg-[#DCEBE9]'
                  : 'text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20]'
              }`}
            >
              {updateInfo?.has_update && <ArrowUpCircle className="h-3 w-3 text-[#2B6B68]" />}
              <span>
                {checkingUpdate
                  ? '检查中...'
                  : updateInfo?.has_update && updateInfo.latest_version
                  ? `升级 v${updateInfo.latest_version}`
                  : '检查更新'}
              </span>
            </button>
          ) : (
            <span>免安装便携版</span>
          )}
        </div>
      </div>
    </div>
  );

  return (
    <>
      <aside className="hidden lg:block lg:sticky lg:top-0 lg:h-screen lg:w-[252px] lg:shrink-0">
        {navContent}
      </aside>
      {mobileOpen && (
        <div className="fixed inset-0 z-50 flex lg:hidden">
          <button
            type="button"
            aria-label="关闭导航遮罩"
            onClick={onCloseMobile}
            className="fixed inset-0 bg-[#181C20]/25 backdrop-blur-[1px]"
          />
          <div className="relative z-10 w-[264px] max-w-[82vw] h-full">{navContent}</div>
        </div>
      )}
    </>
  );
}
