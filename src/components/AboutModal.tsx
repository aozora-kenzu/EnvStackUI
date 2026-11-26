import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  Download,
  ExternalLink,
  Loader2,
  MessageCircle,
  RefreshCw,
  X,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { UpdateCheckResult } from '../types/envstack';

interface AboutModalProps {
  open: boolean;
  version: string;
  rootDir: string;
  wwwRoot: string;
  githubRepo: string;
  autoCheckUpdates: boolean;
  cachedUpdate: UpdateCheckResult | null;
  onUpdateCached: (res: UpdateCheckResult) => void;
  onClose: () => void;
}

const DEV_NAME = 'AKZ Team';
const DEV_EMAIL = 'aozora@botsec.dev';
const DEV_WEBSITE = 'https://botsec.dev';
const DEV_TELEGRAM = 'https://t.me/AozoraKZ';

export function AboutModal({
  open,
  version,
  rootDir,
  wwwRoot,
  githubRepo,
  autoCheckUpdates,
  cachedUpdate,
  onUpdateCached,
  onClose,
}: AboutModalProps) {
  const { t, format } = useI18n();
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<UpdateCheckResult | null>(cachedUpdate);
  const [lastChecked, setLastChecked] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadPct, setDownloadPct] = useState<number | null>(null);
  const [updateStage, setUpdateStage] = useState<'downloading' | 'verifying' | 'installing'>(
    'downloading',
  );
  const [downloadedBytes, setDownloadedBytes] = useState(0);
  const [totalBytes, setTotalBytes] = useState<number | null>(null);
  const [speedBps, setSpeedBps] = useState(0);
  const [downloadErr, setDownloadErr] = useState<string | null>(null);

  const nowTime = () => {
    const d = new Date();
    return d.toTimeString().slice(0, 8);
  };

  const runCheck = async () => {
    setChecking(true);
    try {
      const res = await api.checkForUpdates();
      setResult(res);
      setLastChecked(nowTime());
      onUpdateCached(res);
    } catch (e) {
      setResult({
        status: 'error',
        current_version: version,
        latest_version: null,
        release_name: null,
        release_notes: null,
        release_url: `https://github.com/${githubRepo}/releases`,
        asset_name: null,
        asset_url: null,
        asset_size: null,
        checksum_url: null,
        published_at: null,
        error: String(e),
      });
      setLastChecked(nowTime());
    } finally {
      setChecking(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    setDownloadErr(null);
    setDownloadPct(null);
    if (autoCheckUpdates) {
      void runCheck();
    } else if (cachedUpdate) {
      setResult(cachedUpdate);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const unlistenPromise = listen<{
      stage?: string;
      downloaded: number;
      total: number | null;
      speed_bps?: number;
    }>('update-download-progress', (ev) => {
      const { stage, downloaded, total, speed_bps } = ev.payload;
      if (stage === 'verifying' || stage === 'installing' || stage === 'downloading') {
        setUpdateStage(stage);
      }
      setDownloadedBytes(downloaded);
      setTotalBytes(total);
      setSpeedBps(speed_bps ?? 0);
      if (stage === 'installing') {
        setDownloadPct(100);
      } else if (total && total > 0) {
        setDownloadPct(Math.min(100, Math.round((downloaded / total) * 100)));
      }
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, [open]);

  if (!open) return null;

  const handleInstallAndRestart = async () => {
    if (!result?.asset_url || !result.asset_name) return;
    setDownloading(true);
    setUpdateStage('downloading');
    setDownloadPct(0);
    setDownloadedBytes(0);
    setTotalBytes(result.asset_size ?? null);
    setSpeedBps(0);
    setDownloadErr(null);
    try {
      await api.installAndRestartUpdate(
        result.asset_url,
        result.asset_name,
        result.checksum_url,
      );
    } catch (e) {
      setDownloadErr(String(e));
      setDownloading(false);
    }
  };

  const repoUrl = `https://github.com/${githubRepo}`;

  const renderUpdateStatusLine = () => {
    if (checking) return t.about.checking;
    let base = t.about.notCheckedYet;
    if (result?.status === 'up_to_date') {
      base = format(t.about.upToDate, { version });
    } else if (result?.status === 'update_available') {
      base = format(t.about.updateAvailable, { version: result.latest_version ?? '' });
    } else if (result?.status === 'no_releases') {
      base = t.about.noReleases;
    } else if (result?.status === 'error') {
      base = `${t.about.checkError}${result.error ? `: ${result.error}` : ''}`;
    }
    return lastChecked
      ? `${base} · ${format(t.about.lastChecked, { time: lastChecked })}`
      : base;
  };

  const statusColor = () => {
    if (checking) return 'var(--text-muted)';
    if (result?.status === 'update_available') return 'var(--amber)';
    if (result?.status === 'error') return 'var(--red)';
    if (result?.status === 'up_to_date') return 'var(--text-muted)';
    return 'var(--text-muted)';
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[640px] rounded-lg border overflow-hidden"
        style={{
          background: 'var(--bg-surface)',
          borderColor: 'var(--border)',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div
          className="flex items-center justify-between px-5 py-3.5 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <h2 className="font-semibold text-[15px]" style={{ color: 'var(--text-primary)' }}>
            {t.about.title}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="ui-btn p-1.5"
            aria-label={t.about.close}
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="p-5 flex flex-col gap-3.5 max-h-[80vh] overflow-y-auto">
          {/* Hero Row */}
          <div className="flex items-center gap-3.5">
            <div
              className="w-11 h-11 rounded-lg border flex items-center justify-center shrink-0"
              style={{
                background: 'var(--accent-soft)',
                borderColor: 'var(--accent)',
              }}
            >
              <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
                <rect x="4" y="5.5" width="13" height="2.6" rx="1" fill="var(--accent)" />
                <rect x="7" y="10.7" width="13" height="2.6" rx="1" fill="var(--green)" />
                <rect x="4" y="15.9" width="13" height="2.6" rx="1" fill="var(--accent)" />
              </svg>
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-[18px] font-bold tracking-tight">EnvStackUI</span>
                <span
                  className="font-mono text-[11px] font-semibold px-2 py-0.5 rounded border"
                  style={{
                    background: 'var(--green-soft)',
                    borderColor: 'var(--green)',
                    color: 'var(--green)',
                  }}
                >
                  v{version}
                </span>
              </div>
              <div className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
                {t.about.subtitle}
              </div>
            </div>
          </div>

          {/* Application Update Card */}
          <div
            className="rounded-md border px-3.5 py-3 flex flex-col gap-2.5"
            style={{ background: 'var(--bg-elevated)', borderColor: 'var(--border)' }}
          >
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-[12px] font-semibold" style={{ color: 'var(--text-primary)' }}>
                  {t.about.updateSection}
                </div>
                <div
                  className="font-mono text-[11px] mt-0.5 truncate"
                  style={{ color: statusColor() }}
                  title={renderUpdateStatusLine()}
                >
                  {renderUpdateStatusLine()}
                </div>
              </div>
              <button
                type="button"
                disabled={checking}
                onClick={runCheck}
                className="ui-btn text-[11px] shrink-0"
              >
                <RefreshCw size={12} className={checking ? 'animate-spin' : ''} />
                <span>{t.about.checkNow}</span>
              </button>
            </div>

            {!checking && result?.status === 'update_available' && (
              <div className="flex flex-col gap-2 pt-1 border-t" style={{ borderColor: 'var(--border-subtle)' }}>
                {result.release_notes && (
                  <pre
                    className="font-mono text-[11px] p-2 rounded border max-h-24 overflow-y-auto whitespace-pre-wrap selectable"
                    style={{
                      background: 'var(--bg-input)',
                      borderColor: 'var(--border)',
                      color: 'var(--text-muted)',
                    }}
                  >
                    {result.release_notes}
                  </pre>
                )}

                {downloading && (
                  <div className="flex flex-col gap-1">
                    <div className="flex items-center justify-between text-[10px] font-mono">
                      <span style={{ color: 'var(--accent)' }}>
                        {updateStage === 'verifying'
                          ? t.about.verifyingUpdate
                          : updateStage === 'installing'
                            ? t.about.installingUpdate
                            : `${t.about.downloadingUpdate}${downloadPct !== null ? ` ${downloadPct}%` : ''}`}
                      </span>
                      {updateStage === 'downloading' && downloadedBytes > 0 && (
                        <span style={{ color: 'var(--text-muted)' }}>
                          {(downloadedBytes / (1024 * 1024)).toFixed(1)}
                          {totalBytes ? ` / ${(totalBytes / (1024 * 1024)).toFixed(1)} MB` : ' MB'}
                          {speedBps >= 1024 * 1024
                            ? ` · ${(speedBps / (1024 * 1024)).toFixed(1)} MB/s`
                            : speedBps >= 1024
                              ? ` · ${(speedBps / 1024).toFixed(0)} KB/s`
                              : ''}
                        </span>
                      )}
                    </div>
                    <div
                      className="h-1.5 w-full rounded overflow-hidden"
                      style={{ background: 'var(--bg-input)' }}
                    >
                      <div
                        className="h-full transition-all"
                        style={{
                          width: `${downloadPct ?? 35}%`,
                          background:
                            updateStage === 'installing' ? 'var(--green)' : 'var(--accent)',
                        }}
                      />
                    </div>
                  </div>
                )}

                {downloadErr && (
                  <div className="text-[11px] font-mono selectable" style={{ color: 'var(--red)' }}>
                    {downloadErr}
                  </div>
                )}

                <div className="flex items-center gap-2">
                  {result.asset_url && (
                    <button
                      type="button"
                      disabled={downloading}
                      onClick={handleInstallAndRestart}
                      className="ui-btn ui-btn-primary text-[11px]"
                    >
                      {downloading ? (
                        <Loader2 size={12} className="animate-spin" />
                      ) : (
                        <Download size={12} />
                      )}
                      <span>
                        {downloading
                          ? updateStage === 'verifying'
                            ? t.about.verifyingUpdate
                            : updateStage === 'installing'
                              ? t.about.installingUpdate
                              : `${t.about.downloadingUpdate}${downloadPct !== null ? ` (${downloadPct}%)` : ''}`
                          : t.about.updateAndRestart}
                      </span>
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => api.openExternalUrl(result.release_url)}
                    className="ui-btn text-[11px]"
                  >
                    <ExternalLink size={12} />
                    <span>{t.about.openGithub}</span>
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Description Card */}
          <div
            className="rounded-md border px-3.5 py-2.5 text-[12px] leading-relaxed"
            style={{
              background: 'var(--bg-elevated)',
              borderColor: 'var(--border)',
              color: 'var(--text-muted)',
            }}
          >
            {t.about.description}
          </div>

          {/* Metadata Key-Value Card */}
          <div
            className="rounded-md border px-4 py-3 grid grid-cols-[125px_1fr] gap-y-2 gap-x-3 text-[12px]"
            style={{ background: 'var(--bg-input)', borderColor: 'var(--border)' }}
          >
            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldVersion}</span>
            <span className="font-mono selectable" style={{ color: 'var(--accent)' }}>
              v{version}
            </span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldDeveloper}</span>
            <span className="font-medium">{DEV_NAME}</span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldEmail}</span>
            <span className="font-mono selectable" style={{ color: 'var(--accent)' }}>
              {DEV_EMAIL}
            </span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldWebsite}</span>
            <button
              type="button"
              onClick={() => api.openExternalUrl(DEV_WEBSITE)}
              className="font-mono text-left underline cursor-pointer truncate"
              style={{ color: 'var(--accent)' }}
            >
              {DEV_WEBSITE}
            </button>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldTelegram}</span>
            <button
              type="button"
              onClick={() => api.openExternalUrl(DEV_TELEGRAM)}
              className="font-mono text-left underline cursor-pointer truncate"
              style={{ color: 'var(--accent)' }}
            >
              {DEV_TELEGRAM}
            </button>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldGithub}</span>
            <button
              type="button"
              onClick={() => api.openExternalUrl(repoUrl)}
              className="font-mono text-left underline cursor-pointer truncate"
              style={{ color: 'var(--accent)' }}
            >
              {repoUrl}
            </button>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldLicense}</span>
            <span>Freeware — All Rights Reserved (Copyright © 2026 {DEV_NAME})</span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldBuiltWith}</span>
            <span>Rust + Tauri v2 + React TypeScript</span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldAppFolder}</span>
            <span className="font-mono truncate selectable" style={{ color: 'var(--accent)' }} title={rootDir}>
              {rootDir}
            </span>

            <span style={{ color: 'var(--text-muted)' }}>{t.about.fieldOutputFolder}</span>
            <span className="font-mono truncate selectable" style={{ color: 'var(--accent)' }} title={wwwRoot}>
              {wwwRoot}
            </span>
          </div>
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between px-5 py-3 border-t"
          style={{ borderColor: 'var(--border)', background: 'var(--bg-sidebar)' }}
        >
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => api.openExternalUrl(repoUrl)}
              className="ui-btn text-[12px]"
            >
              <ExternalLink size={12} />
              <span>{t.about.openGithub}</span>
            </button>
            <button
              type="button"
              onClick={() => api.openExternalUrl(DEV_TELEGRAM)}
              className="ui-btn text-[12px]"
            >
              <MessageCircle size={12} />
              <span>{t.about.contactTelegram}</span>
            </button>
          </div>
          <button type="button" onClick={onClose} className="ui-btn ui-btn-primary text-[12px] px-4">
            {t.about.close}
          </button>
        </div>
      </div>
    </div>
  );
}
