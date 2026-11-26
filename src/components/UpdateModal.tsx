import { useEffect, useState } from 'react';
import {
  ArrowUpCircle,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  X,
} from 'lucide-react';
import type {
  UpdateCheckResult,
  UpdateProgressPayload,
  UpdaterSettingsView,
} from '../services/deviceBridge';

interface UpdateModalProps {
  open: boolean;
  onClose: () => void;
  settings: UpdaterSettingsView | null;
  updateInfo: UpdateCheckResult | null;
  checking: boolean;
  updating: boolean;
  progress: UpdateProgressPayload | null;
  errorMessage: string | null;
  onCheckNow: () => void;
  onApplyUpdate: () => void;
  onSaveSettings: (repo: string, token: string | null, clearToken: boolean) => Promise<boolean>;
  onOpenReleaseUrl: (url: string) => void;
}

function formatSizeMb(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return '未知大小';
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(iso: string | null | undefined): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function UpdateModal({
  open,
  onClose,
  settings,
  updateInfo,
  checking,
  updating,
  progress,
  errorMessage,
  onCheckNow,
  onApplyUpdate,
  onSaveSettings,
  onOpenReleaseUrl,
}: UpdateModalProps) {
  const [configExpanded, setConfigExpanded] = useState(false);
  const [repoInput, setRepoInput] = useState(settings?.repo ?? 'NGLS-ls/LumenHID');
  const [tokenInput, setTokenInput] = useState('');
  const [savingSettings, setSavingSettings] = useState(false);

  useEffect(() => {
    if (settings?.repo) {
      setRepoInput(settings.repo);
    }
  }, [settings?.repo]);

  useEffect(() => {
    if (updateInfo?.auth_required) {
      setConfigExpanded(true);
    }
  }, [updateInfo?.auth_required]);

  if (!open) return null;

  const currentVersion = updateInfo?.current_version ?? settings?.current_version ?? '1.0.0';
  const latestVersion = updateInfo?.latest_version ?? updateInfo?.tag_name ?? null;
  const hasUpdate = Boolean(updateInfo?.has_update);
  const canDownload = hasUpdate && Boolean(updateInfo?.download_url);
  const hasToken = Boolean(updateInfo?.has_token ?? settings?.has_token);

  const handleSaveRepoConfig = async () => {
    setSavingSettings(true);
    const trimmedToken = tokenInput.trim();
    const ok = await onSaveSettings(repoInput.trim(), trimmedToken ? trimmedToken : null, false);
    setSavingSettings(false);
    if (ok) {
      setTokenInput('');
      onCheckNow();
    }
  };

  const handleClearToken = async () => {
    setSavingSettings(true);
    const ok = await onSaveSettings(repoInput.trim(), null, true);
    setSavingSettings(false);
    if (ok) {
      setTokenInput('');
      onCheckNow();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#181C20]/35 px-4 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-modal-title"
    >
      <div className="w-full max-w-lg rounded-2xl border border-[#E5E9EE] bg-white shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-[#E5E9EE] px-6 py-4">
          <div className="flex items-center gap-2.5">
            <ArrowUpCircle className="h-5 w-5 text-[#2B6B68]" />
            <div>
              <h2 id="update-modal-title" className="text-base font-semibold text-[#181C20]">
                在线检查与版本升级
              </h2>
              <p className="text-[11px] text-[#84909D]">
                当前版本 v{currentVersion} · 源仓库 {updateInfo?.repo ?? settings?.repo ?? 'NGLS-ls/LumenHID'}
              </p>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            disabled={updating}
            aria-label="关闭更新弹窗"
            className="rounded-lg p-1.5 text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] disabled:opacity-40"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Body */}
        <div className="space-y-4 px-6 py-5">
          {/* Status Summary Banner */}
          <div
            className={`rounded-xl border px-4 py-3 text-xs leading-relaxed ${
              hasUpdate
                ? 'border-[#C4DCD9] bg-[#EBF3F2] text-[#1D4E4C]'
                : 'border-[#E5E9EE] bg-[#F6F7F9] text-[#586370]'
            }`}
          >
            <div className="flex items-center justify-between gap-2 font-medium">
              <span>
                {checking
                  ? '正在连接 GitHub Releases 检查最新版本...'
                  : hasUpdate && latestVersion
                  ? `发现新版本：v${currentVersion} → v${latestVersion}`
                  : updateInfo?.detail ?? '点击下方按钮检查 GitHub Releases 最新版本。'}
              </span>
              {!checking && !hasUpdate && updateInfo && !updateInfo.auth_required && (
                <CheckCircle2 className="h-4 w-4 shrink-0 text-[#1D7A5F]" />
              )}
            </div>
            {hasUpdate && updateInfo?.detail && (
              <p className="mt-1 text-[11px] opacity-90">{updateInfo.detail}</p>
            )}
          </div>

          {/* Release Info & Notes */}
          {updateInfo && (updateInfo.release_title || updateInfo.release_notes) && (
            <div className="rounded-xl border border-[#E5E9EE] bg-[#FAFBFD] p-4 space-y-2.5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-semibold text-[#181C20]">
                  {updateInfo.release_title || updateInfo.tag_name}
                </span>
                {updateInfo.published_at && (
                  <span className="text-[11px] text-[#84909D]">
                    {formatDate(updateInfo.published_at)}
                  </span>
                )}
              </div>

              {updateInfo.release_notes && (
                <div className="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-lg border border-[#EEF1F4] bg-white px-3 py-2 text-xs leading-relaxed text-[#586370]">
                  {updateInfo.release_notes}
                </div>
              )}

              {updateInfo.asset_name && (
                <div className="flex flex-wrap items-center justify-between gap-2 pt-1 text-[11px] text-[#586370]">
                  <span className="font-mono">
                    安装包：{updateInfo.asset_name}（{formatSizeMb(updateInfo.asset_size)}）
                  </span>
                  <span className="inline-flex items-center gap-1 text-[#1D7A5F]">
                    <ShieldCheck className="h-3.5 w-3.5" />
                    <span>
                      {updateInfo.checksum_url
                        ? 'SHA256 + PE 校验 + .bak 自动回滚'
                        : 'PE 文件头校验 + .bak 自动回滚'}
                    </span>
                  </span>
                </div>
              )}
            </div>
          )}

          {/* Download / Verify Progress */}
          {progress && (
            <div className="space-y-2 rounded-xl border border-[#DCE5E4] bg-[#F4F8F7] p-3.5">
              <div className="flex items-center justify-between text-xs font-medium text-[#1D4E4C]">
                <span>{progress.message}</span>
                <span className="font-mono tabular-nums">{progress.percent}%</span>
              </div>
              <div className="h-2 w-full overflow-hidden rounded-full bg-[#DCE5E4]">
                <div
                  className="h-full rounded-full bg-[#2B6B68] transition-all duration-200"
                  style={{ width: `${Math.min(100, Math.max(4, progress.percent))}%` }}
                />
              </div>
            </div>
          )}

          {/* Error Notice */}
          {errorMessage && (
            <div className="rounded-xl border border-[#FECACA] bg-[#FEF2F2] px-4 py-2.5 text-xs text-[#991B1B]">
              {errorMessage}
            </div>
          )}

          {/* Repo & Private Token Config */}
          <div className="rounded-xl border border-[#E5E9EE]">
            <button
              type="button"
              onClick={() => setConfigExpanded((prev) => !prev)}
              className="flex w-full items-center justify-between px-4 py-2.5 text-xs font-medium text-[#586370] hover:text-[#181C20]"
            >
              <span className="inline-flex items-center gap-1.5">
                <KeyRound className="h-3.5 w-3.5" />
                <span>
                  GitHub 仓库与私有 Token 设置{hasToken ? '（已配置 Token）' : ''}
                </span>
              </span>
              {configExpanded ? (
                <ChevronUp className="h-3.5 w-3.5" />
              ) : (
                <ChevronDown className="h-3.5 w-3.5" />
              )}
            </button>

            {configExpanded && (
              <div className="space-y-3 border-t border-[#E5E9EE] px-4 py-3 text-xs">
                <div>
                  <label className="mb-1 block text-[11px] font-medium text-[#586370]">
                    GitHub 仓库（owner/repo）
                  </label>
                  <input
                    type="text"
                    value={repoInput}
                    onChange={(e) => setRepoInput(e.target.value)}
                    placeholder="NGLS-ls/LumenHID"
                    className="w-full rounded-lg border border-[#E5E9EE] px-3 py-1.5 font-mono text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                  />
                </div>

                <div>
                  <label className="mb-1 block text-[11px] font-medium text-[#586370]">
                    GitHub Token（仅私有仓库需要，保存在本地 LumenHID.data/updater.json）
                  </label>
                  <input
                    type="password"
                    value={tokenInput}
                    onChange={(e) => setTokenInput(e.target.value)}
                    placeholder={hasToken ? '已保存 Token（留空保持不变，输入新值可覆盖）' : '可选，填入 github_pat_...'}
                    className="w-full rounded-lg border border-[#E5E9EE] px-3 py-1.5 font-mono text-xs text-[#181C20] focus:border-[#2B6B68] focus:outline-none"
                  />
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    disabled={savingSettings || updating}
                    onClick={() => void handleSaveRepoConfig()}
                    className="rounded-lg bg-[#181C20] px-3 py-1.5 text-[11px] font-medium text-white hover:bg-[#2C333B] disabled:opacity-50"
                  >
                    {savingSettings ? '保存中...' : '保存并检查'}
                  </button>
                  {hasToken && (
                    <button
                      type="button"
                      disabled={savingSettings || updating}
                      onClick={() => void handleClearToken()}
                      className="rounded-lg border border-[#E5E9EE] bg-white px-3 py-1.5 text-[11px] font-medium text-[#586370] hover:bg-[#F6F7F9] disabled:opacity-50"
                    >
                      清除已存 Token
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer Actions */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[#E5E9EE] bg-[#FAFBFD] px-6 py-3.5">
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={checking || updating}
              onClick={onCheckNow}
              className="inline-flex items-center gap-1.5 rounded-lg border border-[#E5E9EE] bg-white px-3 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20] disabled:opacity-50"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${checking ? 'animate-spin' : ''}`} />
              <span>{checking ? '检查中...' : '重新检查'}</span>
            </button>

            {updateInfo?.release_url && (
              <button
                type="button"
                onClick={() => onOpenReleaseUrl(updateInfo.release_url!)}
                className="inline-flex items-center gap-1 rounded-lg border border-[#E5E9EE] bg-white px-3 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] hover:text-[#181C20]"
              >
                <ExternalLink className="h-3.5 w-3.5" />
                <span>Release 页</span>
              </button>
            )}
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={updating}
              onClick={onClose}
              className="rounded-lg border border-[#E5E9EE] bg-white px-3.5 py-2 text-xs font-medium text-[#586370] hover:bg-[#F0F3F6] disabled:opacity-50"
            >
              关闭
            </button>

            {canDownload && (
              <button
                type="button"
                disabled={updating || checking}
                onClick={onApplyUpdate}
                className="inline-flex items-center gap-1.5 rounded-lg bg-[#2B6B68] px-4 py-2 text-xs font-medium text-white hover:bg-[#215452] disabled:opacity-50"
              >
                <ArrowUpCircle className="h-3.5 w-3.5" />
                <span>{updating ? '正在拉取并升级...' : '立即拉取并升级'}</span>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
