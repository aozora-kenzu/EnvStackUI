import { useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  Check,
  Download,
  FileArchive,
  FolderOpen,
  Link2,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { AppBootstrap, DownloadProgress, OnlineVersionItem } from '../types/envstack';

interface PackagesViewProps {
  bootstrap: AppBootstrap;
  installingIds: string[];
  progressMap: Record<string, DownloadProgress>;
  onInstallPackage: (item: OnlineVersionItem) => Promise<void>;
  onInstallCustomArchive: (
    kind: string,
    versionName: string,
    source: string,
    isLocalFile: boolean,
  ) => Promise<boolean>;
  onUninstalled: (updated: AppBootstrap) => void;
  onError: (msg: string) => void;
  onOpenBinDir: () => void;
}

const CATEGORIES: { id: string; label: string; hint: string }[] = [
  { id: 'php', label: 'PHP', hint: 'windows.php.net' },
  { id: 'node', label: 'Node.js', hint: 'nodejs.org' },
  { id: 'python', label: 'Python', hint: 'astral-sh/python-build-standalone' },
  { id: 'nginx', label: 'Nginx', hint: 'nginx.org' },
  { id: 'apache', label: 'Apache', hint: 'apachelounge.com' },
  { id: 'mysql', label: 'MariaDB', hint: 'mariadb.org' },
  { id: 'go', label: 'Go', hint: 'go.dev' },
  { id: 'mailpit', label: 'Mailpit', hint: 'axllent/mailpit' },
  { id: 'composer', label: 'Composer', hint: 'getcomposer.org' },
  { id: 'adminer', label: 'Adminer', hint: 'vrana/adminer' },
];

type FilterKey =
  | 'all'
  | 'installed'
  | 'nts'
  | 'ts'
  | 'lts'
  | 'current'
  | 'stable'
  | 'prerelease'
  | 'mainline';

function filtersForCategory(kind: string): FilterKey[] {
  switch (kind) {
    case 'php':
      return ['all', 'nts', 'ts', 'installed'];
    case 'node':
      return ['all', 'lts', 'current', 'installed'];
    case 'python':
      return ['all', 'stable', 'prerelease', 'installed'];
    case 'nginx':
      return ['all', 'stable', 'mainline', 'installed'];
    case 'composer':
      return ['all', 'stable', 'lts', 'installed'];
    default:
      return ['all', 'installed'];
  }
}

function formatSpeed(bps: number): string {
  if (bps <= 0) return '';
  if (bps >= 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`;
  if (bps >= 1024) return `${(bps / 1024).toFixed(0)} KB/s`;
  return `${bps} B/s`;
}

function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || seconds <= 0) return '';
  if (seconds < 60) return `~${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const rem = seconds % 60;
  return `~${mins}m ${rem}s`;
}

// Module-level session cache so switching tabs away from Packages and back never resets or re-fetches
let cachedVersionsByKind: Record<string, OnlineVersionItem[]> = {};
let cachedSelectedKind = 'php';

export function PackagesView({
  bootstrap,
  installingIds,
  progressMap,
  onInstallPackage,
  onInstallCustomArchive,
  onUninstalled,
  onError,
  onOpenBinDir,
}: PackagesViewProps) {
  const { t, format } = useI18n();
  const [selectedKind, setSelectedKind] = useState<string>(cachedSelectedKind);
  const [activeFilter, setActiveFilter] = useState<FilterKey>('all');
  const [versionsByKind, setVersionsByKind] =
    useState<Record<string, OnlineVersionItem[]>>(cachedVersionsByKind);
  const [loadingKind, setLoadingKind] = useState<boolean>(false);
  const [fetchErr, setFetchErr] = useState<string | null>(null);
  const [search, setSearch] = useState<string>('');
  const [confirmUninstallId, setConfirmUninstallId] = useState<string | null>(null);
  const [uninstallingId, setUninstallingId] = useState<string | null>(null);
  const customBusy = installingIds.includes('custom');

  // Custom installer modal state
  const [customOpen, setCustomOpen] = useState(false);
  const [customMode, setCustomMode] = useState<'url' | 'file'>('url');
  const [customKind, setCustomKind] = useState<string>('node');
  const [customVersion, setCustomVersion] = useState<string>('');
  const [customSource, setCustomSource] = useState<string>('');

  const loadCategory = async (kind: string, force = false) => {
    setLoadingKind(true);
    setFetchErr(null);
    try {
      const items = await api.fetchOnlineVersions(kind, force);
      cachedVersionsByKind = { ...cachedVersionsByKind, [kind]: items };
      setVersionsByKind(cachedVersionsByKind);
    } catch (e) {
      setFetchErr(String(e));
    } finally {
      setLoadingKind(false);
    }
  };

  useEffect(() => {
    cachedSelectedKind = selectedKind;
    if (!cachedVersionsByKind[selectedKind]) {
      void loadCategory(selectedKind, false);
    } else {
      setFetchErr(null);
    }
  }, [selectedKind]);

  const isPortableInstalled = (item: OnlineVersionItem) => {
    if (item.kind === 'adminer') return bootstrap.adminer_installed;
    const target = item.target_folder.trim().toLowerCase();
    const bins = bootstrap.inventory.binaries[item.kind] ?? [];
    return bins.some((b) => {
      if (b.source !== 'portable') return false;
      const normRoot = b.root_dir.replace(/[\\/]+$/, '').toLowerCase();
      return (
        b.name.toLowerCase() === target ||
        normRoot.endsWith(`\\${target}`) ||
        normRoot.endsWith(`/${target}`)
      );
    });
  };

  const handleInstall = (item: OnlineVersionItem) => {
    setConfirmUninstallId(null);
    void onInstallPackage(item);
  };

  const handleUninstall = async (item: OnlineVersionItem) => {
    setUninstallingId(item.id);
    try {
      const updated = await api.uninstallPackage(item.kind, item.target_folder);
      onUninstalled(updated);
      setConfirmUninstallId(null);
    } catch (e) {
      onError(String(e));
    } finally {
      setUninstallingId(null);
    }
  };

  const handleCustomInstall = async () => {
    if (!customSource.trim()) return;
    const ok = await onInstallCustomArchive(
      customKind,
      customVersion,
      customSource,
      customMode === 'file',
    );
    if (ok) {
      setCustomOpen(false);
      setCustomVersion('');
      setCustomSource('');
    }
  };

  const pickCustomFile = async () => {
    const file = await api.pickArchiveFile(t.packages.customModalTitle);
    if (file) setCustomSource(file);
  };

  const items = versionsByKind[selectedKind] ?? [];
  const availableFilters = filtersForCategory(selectedKind);

  const matchesFilter = (item: OnlineVersionItem, key: FilterKey) => {
    if (key === 'all') return true;
    if (key === 'installed') return isPortableInstalled(item);
    return item.tags.includes(key);
  };

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return items.filter((it) => {
      if (!matchesFilter(it, activeFilter)) return false;
      if (!q) return true;
      return (
        it.name.toLowerCase().includes(q) ||
        it.version.toLowerCase().includes(q) ||
        (it.variant?.toLowerCase().includes(q) ?? false)
      );
    });
  }, [items, activeFilter, search, bootstrap.inventory.binaries, bootstrap.adminer_installed]);

  const activeCategoryMeta = CATEGORIES.find((c) => c.id === selectedKind);

  return (
    <div className="flex flex-col gap-4 pb-12">
      {/* Header */}
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.packages.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.packages.subtitle}
          </p>
        </div>
        <div className="flex items-center gap-1.5 flex-wrap">
          <button
            type="button"
            onClick={() => {
              setCustomKind(selectedKind);
              setCustomOpen(true);
            }}
            className="ui-btn ui-btn-primary text-[11px]"
          >
            <Plus size={12} />
            <span>{t.packages.customInstallBtn}</span>
          </button>
          <button
            type="button"
            disabled={loadingKind}
            onClick={() => void loadCategory(selectedKind, true)}
            className="ui-btn text-[11px]"
          >
            <RefreshCw size={12} className={loadingKind ? 'animate-spin' : ''} />
            <span>{t.packages.refreshVersions}</span>
          </button>
          <button type="button" onClick={onOpenBinDir} className="ui-btn text-[11px]">
            <FolderOpen size={12} />
            <span>{t.packages.openBinDir}</span>
          </button>
        </div>
      </div>

      {/* Category Tabs */}
      <div className="flex items-center gap-1.5 flex-wrap">
        {CATEGORIES.map((cat) => {
          const active = selectedKind === cat.id;
          const localCount =
            cat.id === 'adminer'
              ? bootstrap.adminer_installed
                ? 1
                : 0
              : (bootstrap.inventory.binaries[cat.id] ?? []).filter(
                  (b) => b.source === 'portable',
                ).length;
          return (
            <button
              key={cat.id}
              type="button"
              title={
                localCount > 0
                  ? format(t.packages.installedInBinHint, { count: localCount, kind: cat.id })
                  : undefined
              }
              onClick={() => {
                setSelectedKind(cat.id);
                setActiveFilter('all');
                setSearch('');
                setConfirmUninstallId(null);
              }}
              className="px-2.5 py-1 rounded border text-[11px] font-medium cursor-pointer flex items-center gap-1.5 transition-colors"
              style={{
                background: active ? 'var(--bg-active)' : 'var(--bg-surface)',
                borderColor: active ? 'var(--accent)' : 'var(--border)',
                color: active ? 'var(--text-primary)' : 'var(--text-muted)',
              }}
            >
              <span>{cat.label}</span>
              {localCount > 0 && (
                <span
                  className="font-mono text-[10px] px-1 rounded"
                  style={{ background: 'var(--green-soft)', color: 'var(--green)' }}
                >
                  {localCount}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Search + Variant Filter Pills + Upstream Info */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div className="relative flex-1 min-w-[220px] max-w-[340px]">
            <Search
              size={12}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
              style={{ color: 'var(--text-dim)' }}
            />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t.packages.searchPlaceholder}
              className="ui-input w-full pl-8 font-mono text-[11px]"
            />
          </div>

          <div className="flex items-center gap-1 flex-wrap">
            {availableFilters.map((fKey) => {
              const active = activeFilter === fKey;
              const count = items.filter((it) => matchesFilter(it, fKey)).length;
              return (
                <button
                  key={fKey}
                  type="button"
                  onClick={() => setActiveFilter(fKey)}
                  className="px-2 py-1 rounded border text-[11px] cursor-pointer flex items-center gap-1 transition-colors"
                  style={{
                    background: active ? 'var(--accent-soft)' : 'var(--bg-surface)',
                    borderColor: active ? 'var(--accent)' : 'var(--border)',
                    color: active ? 'var(--accent)' : 'var(--text-muted)',
                  }}
                >
                  <span>{t.packages.filters[fKey]}</span>
                  {items.length > 0 && (
                    <span className="font-mono text-[10px] opacity-75">({count})</span>
                  )}
                </button>
              );
            })}
          </div>
        </div>

        {activeCategoryMeta && (
          <div className="font-mono text-[11px]" style={{ color: 'var(--text-dim)' }}>
            Upstream: <span style={{ color: 'var(--accent)' }}>{activeCategoryMeta.hint}</span>
          </div>
        )}
      </div>

      {/* States */}
      {loadingKind && items.length === 0 ? (
        <div
          className="ui-panel p-8 flex items-center justify-center gap-2 text-[12px]"
          style={{ color: 'var(--text-muted)' }}
        >
          <Loader2 size={15} className="animate-spin" style={{ color: 'var(--accent)' }} />
          <span>{t.packages.fetchingVersions}</span>
        </div>
      ) : fetchErr ? (
        <div
          className="ui-panel p-4 flex items-center justify-between gap-3"
          style={{ borderColor: 'var(--red)', background: 'var(--red-soft)' }}
        >
          <div className="flex items-start gap-2 text-[12px]" style={{ color: 'var(--red)' }}>
            <AlertCircle size={15} className="shrink-0 mt-0.5" />
            <div>
              <div className="font-semibold">{t.packages.fetchError}</div>
              <div className="font-mono text-[11px] mt-0.5 selectable">{fetchErr}</div>
            </div>
          </div>
          <button
            type="button"
            onClick={() => void loadCategory(selectedKind, true)}
            className="ui-btn text-[11px] shrink-0"
          >
            <RefreshCw size={12} />
            <span>{t.packages.refreshVersions}</span>
          </button>
        </div>
      ) : filtered.length === 0 ? (
        <div className="ui-panel p-8 text-center text-[12px]" style={{ color: 'var(--text-dim)' }}>
          {t.packages.noMatchingVersions}
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
          {filtered.map((item) => {
            const portableInstalled = isPortableInstalled(item);
            const prog = progressMap[item.id];
            const busy = installingIds.includes(item.id);
            const removing = uninstallingId === item.id;
            const confirming = confirmUninstallId === item.id;
            const showProgress =
              Boolean(prog) &&
              (busy ||
                prog?.stage === 'downloading' ||
                prog?.stage === 'extracting' ||
                prog?.stage === 'error');
            const pct =
              prog?.total && prog.total > 0
                ? Math.min(100, Math.round((prog.downloaded / prog.total) * 100))
                : null;
            const speedStr = prog ? formatSpeed(prog.speed_bps) : '';
            const etaStr = prog ? formatEta(prog.eta_seconds) : '';

            return (
              <div key={item.id} className="ui-panel p-3.5 flex flex-col justify-between gap-2.5">
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="font-semibold text-[13px] truncate">{item.name}</div>
                    <div
                      className="font-mono text-[10px] mt-0.5 truncate"
                      style={{ color: 'var(--text-dim)' }}
                    >
                      {item.source_label} · bin/{item.kind}/{item.target_folder}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    {item.variant && (
                      <span
                        className="font-mono text-[10px] px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
                      >
                        {item.variant}
                      </span>
                    )}
                    {portableInstalled && (
                      <span
                        className="font-mono text-[10px] px-1.5 py-0.5 rounded"
                        style={{ background: 'var(--green-soft)', color: 'var(--green)' }}
                      >
                        {t.packages.installed}
                      </span>
                    )}
                  </div>
                </div>

                {showProgress && prog && (
                  <div className="flex flex-col gap-1">
                    <div className="flex items-center justify-between gap-2 text-[10px] font-mono">
                      <span
                        style={{
                          color: prog.stage === 'error' ? 'var(--red)' : 'var(--accent)',
                        }}
                      >
                        {prog.stage === 'downloading'
                          ? `${t.packages.downloading}${pct !== null ? ` ${pct}%` : ''}`
                          : prog.stage === 'extracting'
                            ? t.packages.extracting
                            : prog.stage === 'error'
                              ? prog.message ?? 'Error'
                              : t.packages.done}
                      </span>
                      {prog.downloaded > 0 && prog.stage === 'downloading' && (
                        <span className="truncate" style={{ color: 'var(--text-muted)' }}>
                          {(prog.downloaded / (1024 * 1024)).toFixed(1)}
                          {prog.total
                            ? ` / ${(prog.total / (1024 * 1024)).toFixed(1)} MB`
                            : ' MB'}
                          {speedStr ? ` · ${speedStr}` : ''}
                          {etaStr ? ` · ${etaStr}` : ''}
                        </span>
                      )}
                    </div>
                    {prog.stage !== 'error' && (
                      <div
                        className="h-1 w-full rounded overflow-hidden"
                        style={{ background: 'var(--bg-elevated)' }}
                      >
                        <div
                          className="h-full transition-all"
                          style={{
                            width: `${pct ?? (prog.stage === 'extracting' ? 95 : 35)}%`,
                            background: 'var(--accent)',
                          }}
                        />
                      </div>
                    )}
                  </div>
                )}

                <div className="flex items-center justify-between gap-2 pt-0.5">
                  <span className="font-mono text-[10px]" style={{ color: 'var(--text-dim)' }}>
                    .{item.archive_type}
                  </span>

                  <div className="flex items-center gap-1.5 flex-wrap justify-end">
                    {item.size_label && (
                      <span
                        className="font-mono text-[11px] px-1.5 py-0.5 rounded border"
                        style={{
                          background: 'var(--bg-elevated)',
                          borderColor: 'var(--border)',
                          color: 'var(--text-muted)',
                        }}
                      >
                        {item.size_label}
                      </span>
                    )}

                    {portableInstalled && !confirming && (
                      <button
                        type="button"
                        disabled={busy || removing}
                        onClick={() => setConfirmUninstallId(item.id)}
                        title={t.packages.uninstall}
                        className="ui-btn ui-btn-danger text-[11px] px-2"
                      >
                        <Trash2 size={12} />
                        <span>{t.packages.uninstall}</span>
                      </button>
                    )}

                    {portableInstalled && confirming && (
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          disabled={removing}
                          onClick={() => handleUninstall(item)}
                          className="ui-btn ui-btn-danger text-[11px] px-2"
                        >
                          {removing ? (
                            <Loader2 size={12} className="animate-spin" />
                          ) : (
                            <Trash2 size={12} />
                          )}
                          <span>
                            {removing ? t.packages.uninstalling : t.packages.confirmUninstall}
                          </span>
                        </button>
                        <button
                          type="button"
                          disabled={removing}
                          onClick={() => setConfirmUninstallId(null)}
                          title={t.packages.cancel}
                          className="ui-btn text-[11px] p-1.5"
                        >
                          <X size={11} />
                        </button>
                      </div>
                    )}

                    {!confirming && (
                      <button
                        type="button"
                        disabled={busy || removing}
                        onClick={() => handleInstall(item)}
                        className={
                          portableInstalled
                            ? 'ui-btn text-[11px]'
                            : 'ui-btn ui-btn-primary text-[11px]'
                        }
                      >
                        {busy ? (
                          <Loader2 size={12} className="animate-spin" />
                        ) : portableInstalled ? (
                          <Check size={12} />
                        ) : (
                          <Download size={12} />
                        )}
                        <span>{portableInstalled ? t.packages.reinstall : t.packages.install}</span>
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Custom URL / Local Archive Modal */}
      {customOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4"
          onClick={() => !customBusy && setCustomOpen(false)}
        >
          <div
            className="w-full max-w-[500px] rounded-lg border overflow-hidden"
            style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="flex items-center justify-between px-4 py-3 border-b"
              style={{ borderColor: 'var(--border)' }}
            >
              <h2 className="font-semibold text-[14px]">{t.packages.customModalTitle}</h2>
              <button
                type="button"
                disabled={customBusy}
                onClick={() => setCustomOpen(false)}
                className="ui-btn p-1"
              >
                <X size={14} />
              </button>
            </div>

            <div className="p-4 flex flex-col gap-3">
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => {
                    setCustomMode('url');
                    setCustomSource('');
                  }}
                  className={
                    customMode === 'url' ? 'ui-btn ui-btn-primary text-[12px]' : 'ui-btn text-[12px]'
                  }
                >
                  <Link2 size={13} />
                  <span>{t.packages.customModeUrl}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setCustomMode('file');
                    setCustomSource('');
                  }}
                  className={
                    customMode === 'file' ? 'ui-btn ui-btn-primary text-[12px]' : 'ui-btn text-[12px]'
                  }
                >
                  <FileArchive size={13} />
                  <span>{t.packages.customModeFile}</span>
                </button>
              </div>

              <label className="flex flex-col gap-1 text-[12px]">
                <span style={{ color: 'var(--text-muted)' }}>{t.packages.customKindLabel}</span>
                <select
                  value={customKind}
                  onChange={(e) => setCustomKind(e.target.value)}
                  className="ui-select font-mono"
                >
                  {CATEGORIES.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.label} (bin/{c.id})
                    </option>
                  ))}
                  <option value="git">Git (bin/git)</option>
                </select>
              </label>

              <label className="flex flex-col gap-1 text-[12px]">
                <span style={{ color: 'var(--text-muted)' }}>{t.packages.customVersionLabel}</span>
                <input
                  type="text"
                  value={customVersion}
                  onChange={(e) => setCustomVersion(e.target.value)}
                  placeholder={t.packages.customVersionPlaceholder}
                  className="ui-input font-mono"
                />
              </label>

              {customMode === 'url' ? (
                <label className="flex flex-col gap-1 text-[12px]">
                  <span style={{ color: 'var(--text-muted)' }}>URL (.zip / .tar.gz)</span>
                  <input
                    type="text"
                    value={customSource}
                    onChange={(e) => setCustomSource(e.target.value)}
                    placeholder={t.packages.customUrlPlaceholder}
                    className="ui-input font-mono"
                  />
                </label>
              ) : (
                <div className="flex flex-col gap-1 text-[12px]">
                  <span style={{ color: 'var(--text-muted)' }}>{t.packages.customModeFile}</span>
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      value={customSource}
                      onChange={(e) => setCustomSource(e.target.value)}
                      placeholder="C:\Downloads\python-3.14.tar.gz"
                      className="ui-input font-mono flex-1"
                    />
                    <button type="button" onClick={pickCustomFile} className="ui-btn text-[11px]">
                      {t.packages.customPickFile}
                    </button>
                  </div>
                </div>
              )}

              {progressMap['custom'] && customBusy && (
                <div className="text-[11px] font-mono" style={{ color: 'var(--accent)' }}>
                  {progressMap['custom'].stage === 'downloading'
                    ? `${t.packages.downloading} ${(progressMap['custom'].downloaded / (1024 * 1024)).toFixed(1)} MB` +
                      (progressMap['custom'].speed_bps > 0
                        ? ` · ${formatSpeed(progressMap['custom'].speed_bps)}`
                        : '') +
                      (formatEta(progressMap['custom'].eta_seconds)
                        ? ` · ${formatEta(progressMap['custom'].eta_seconds)}`
                        : '')
                    : t.packages.extracting}
                </div>
              )}
            </div>

            <div
              className="flex justify-end gap-2 px-4 py-3 border-t"
              style={{ borderColor: 'var(--border)', background: 'var(--bg-sidebar)' }}
            >
              <button
                type="button"
                disabled={customBusy}
                onClick={() => setCustomOpen(false)}
                className="ui-btn"
              >
                {t.about.close}
              </button>
              <button
                type="button"
                disabled={customBusy || !customSource.trim()}
                onClick={handleCustomInstall}
                className="ui-btn ui-btn-primary"
              >
                {customBusy ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <Download size={13} />
                )}
                <span>{t.packages.customInstallSubmit}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
