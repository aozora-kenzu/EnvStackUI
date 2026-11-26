import {
  Database,
  ExternalLink,
  FolderPlus,
  Globe,
  Loader2,
  Package,
  Play,
  RefreshCw,
  RotateCw,
  SlidersHorizontal,
  Square,
  Unlink,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap, BinaryItem, ServiceStatus } from '../types/envstack';

interface ServicesViewProps {
  bootstrap: AppBootstrap;
  busyService: string | null;
  rescanning: boolean;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onRestart: (id: string) => void;
  onSelectVersion: (kind: string, id: string) => void;
  onGoPackages: () => void;
  onLinkExternalRoot: () => void;
  onLinkServiceBinary: (kind: string) => void;
  onRemoveExternalRoot: (path: string) => void;
  onRescan: () => void;
  onOpenAdminer: () => void;
  onLaunchDbClient: () => void;
  onOpenPhpExtensions: () => void;
  onOpenUrl: (url: string) => void;
}

export function ServicesView({
  bootstrap,
  busyService,
  rescanning,
  onStart,
  onStop,
  onRestart,
  onSelectVersion,
  onGoPackages,
  onLinkExternalRoot,
  onLinkServiceBinary,
  onRemoveExternalRoot,
  onRescan,
  onOpenAdminer,
  onLaunchDbClient,
  onOpenPhpExtensions,
  onOpenUrl,
}: ServicesViewProps) {
  const { t } = useI18n();
  const anyInstalled = bootstrap.services.some((s) => s.installed);

  const findMatchingExternalRoot = (item: BinaryItem | undefined): string | null => {
    if (!item || item.source !== 'external') return null;
    const normItem = item.root_dir.replace(/[\\/]+$/, '').toLowerCase();
    for (const root of bootstrap.config.external_bin_roots) {
      const normRoot = root.replace(/[\\/]+$/, '').toLowerCase();
      if (normItem === normRoot || normItem.startsWith(`${normRoot}\\`)) {
        return root;
      }
    }
    return null;
  };

  const stateColor = (state: ServiceStatus['state']) => {
    switch (state) {
      case 'running':
        return 'var(--green)';
      case 'starting':
        return 'var(--amber)';
      case 'error':
        return 'var(--red)';
      default:
        return 'var(--text-dim)';
    }
  };

  const stateBg = (state: ServiceStatus['state']) => {
    switch (state) {
      case 'running':
        return 'var(--green-soft)';
      case 'starting':
        return 'var(--amber-soft)';
      case 'error':
        return 'var(--red-soft)';
      default:
        return 'var(--bg-elevated)';
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.services.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.services.subtitle}
          </p>
        </div>

        <div className="flex items-center gap-1.5 flex-wrap">
          <button
            type="button"
            onClick={onLinkExternalRoot}
            className="ui-btn text-[11px]"
          >
            <FolderPlus size={12} />
            <span>{t.services.linkExisting}</span>
          </button>
          <button
            type="button"
            onClick={onGoPackages}
            className="ui-btn text-[11px]"
          >
            <Package size={12} />
            <span>{t.services.goToPackages}</span>
          </button>
          <button
            type="button"
            disabled={rescanning}
            onClick={onRescan}
            className="ui-btn text-[11px]"
          >
            <RefreshCw size={12} className={rescanning ? 'animate-spin' : ''} />
            <span>{t.sidebar.rescan}</span>
          </button>
        </div>
      </div>

      {!anyInstalled && (
        <div
          className="ui-panel p-4 flex flex-col gap-3"
          style={{ borderColor: 'var(--accent)' }}
        >
          <div>
            <div className="text-[13px] font-semibold" style={{ color: 'var(--text-primary)' }}>
              {t.services.noBinariesTitle}
            </div>
            <p className="text-[12px] mt-1" style={{ color: 'var(--text-muted)' }}>
              {t.services.noBinariesDesc}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onGoPackages}
              className="ui-btn ui-btn-primary"
            >
              <Package size={13} />
              <span>{t.services.goToPackages}</span>
            </button>
            <button
              type="button"
              onClick={onLinkExternalRoot}
              className="ui-btn"
            >
              <FolderPlus size={13} />
              <span>{t.services.linkExisting}</span>
            </button>
          </div>
        </div>
      )}

      {/* Compact inline Linked External Binary Folders bar */}
      {bootstrap.config.external_bin_roots.length > 0 && (
        <div className="ui-panel px-3.5 py-2 flex items-center flex-wrap gap-2">
          <span className="text-[11px] font-semibold shrink-0" style={{ color: 'var(--text-muted)' }}>
            {t.services.linkedFoldersTitle}:
          </span>
          <div className="flex flex-wrap gap-1.5">
            {bootstrap.config.external_bin_roots.map((root) => (
              <div
                key={root}
                className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded border text-[11px]"
                style={{ background: 'var(--bg-elevated)', borderColor: 'var(--border)' }}
              >
                <span className="font-mono selectable">{root}</span>
                <button
                  type="button"
                  onClick={() => onRemoveExternalRoot(root)}
                  title={`${t.services.unlinkFolder}: ${root}`}
                  className="ui-btn ui-btn-danger text-[10px] py-0.5 px-1.5"
                >
                  <Unlink size={10} />
                  <span>{t.services.unlinkFolder}</span>
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Uniform 3-Column Aligned Service Rows */}
      <div className="flex flex-col gap-2">
        {bootstrap.services.map((svc) => {
          const versions = bootstrap.inventory.binaries[svc.id] ?? [];
          const activeId = bootstrap.config.active_versions[svc.id] ?? '';
          const activeBin = versions.find((v) => v.id === activeId);
          const matchingExtRoot = findMatchingExternalRoot(activeBin);
          const isRunning = svc.state === 'running';
          const isBusy = busyService === svc.id;

          return (
            <div
              key={svc.id}
              className="ui-panel px-4 py-3 flex flex-col gap-2"
              style={{
                borderColor: isRunning ? 'var(--green)' : 'var(--border)',
              }}
            >
              <div className="grid grid-cols-1 md:grid-cols-[270px_minmax(220px,1fr)_136px] items-center gap-3">
                {/* Col 1: status dot + name + badges */}
                <div className="flex items-center gap-3 min-w-0">
                  <span
                    className="w-2.5 h-2.5 rounded-full shrink-0"
                    style={{ background: stateColor(svc.state) }}
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-[13px] truncate">{svc.label}</span>
                      <span
                        className="font-mono text-[10px] px-1.5 py-0.2 rounded shrink-0"
                        style={{
                          background: stateBg(svc.state),
                          color: stateColor(svc.state),
                        }}
                      >
                        {t.services.status[svc.state]}
                      </span>
                      {svc.pid && (
                        <span
                          className="font-mono text-[10px] shrink-0"
                          style={{ color: 'var(--text-dim)' }}
                        >
                          {t.services.pid} {svc.pid}
                        </span>
                      )}
                    </div>
                    <div
                      className="flex items-center gap-2 text-[11px] font-mono mt-0.5"
                      style={{ color: 'var(--text-muted)' }}
                    >
                      <span>
                        {t.services.port}: {svc.port}
                        {svc.secondary_port ? ` / ${svc.secondary_port}` : ''}
                      </span>
                      {svc.source && (
                        <span
                          className="px-1 rounded text-[10px]"
                          style={{ background: 'var(--bg-elevated)', color: 'var(--text-dim)' }}
                        >
                          {svc.source === 'portable'
                            ? t.services.sourcePortable
                            : t.services.sourceExternal}
                        </span>
                      )}
                    </div>
                  </div>
                </div>

                {/* Col 2: aligned version selector + link / unlink buttons */}
                <div className="flex items-center gap-1.5 min-w-0">
                  {versions.length > 0 ? (
                    <select
                      value={activeId}
                      disabled={isRunning || isBusy}
                      onChange={(e) => onSelectVersion(svc.id, e.target.value)}
                      className="ui-select font-mono min-w-0 flex-1 text-[11px]"
                    >
                      {versions.map((v) => (
                        <option key={v.id} value={v.id}>
                          {v.version}
                          {v.variant ? ` (${v.variant})` : ''} [{v.source}]
                        </option>
                      ))}
                    </select>
                  ) : (
                    <span
                      className="text-[11px] font-mono flex-1 truncate"
                      style={{ color: 'var(--text-dim)' }}
                    >
                      {t.services.notInstalled}
                    </span>
                  )}

                  <button
                    type="button"
                    disabled={isRunning || isBusy}
                    onClick={() => onLinkServiceBinary(svc.id)}
                    title={t.services.linkForService}
                    aria-label={t.services.linkForService}
                    className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                  >
                    <FolderPlus size={13} />
                  </button>

                  {matchingExtRoot && (
                    <button
                      type="button"
                      disabled={isRunning || isBusy}
                      onClick={() => onRemoveExternalRoot(matchingExtRoot)}
                      title={`${t.services.unlinkServiceBinary} (${matchingExtRoot})`}
                      aria-label={t.services.unlinkServiceBinary}
                      className="ui-btn ui-btn-danger w-7 h-7 p-0 flex items-center justify-center shrink-0"
                    >
                      <Unlink size={13} />
                    </button>
                  )}
                </div>

                {/* Col 3: right-aligned icon-only action buttons with tooltips */}
                <div className="flex items-center justify-end gap-1.5">
                  {(svc.id === 'nginx' || svc.id === 'apache') && isRunning && (
                    <button
                      type="button"
                      onClick={() =>
                        onOpenUrl(
                          svc.port === 80 ? 'http://localhost' : `http://localhost:${svc.port}`,
                        )
                      }
                      title={`${t.services.openLocalhost} (${
                        svc.port === 80 ? 'http://localhost' : `http://localhost:${svc.port}`
                      })`}
                      aria-label={t.services.openLocalhost}
                      className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                    >
                      <Globe size={13} style={{ color: 'var(--accent)' }} />
                    </button>
                  )}

                  {svc.id === 'php' && svc.installed && (
                    <button
                      type="button"
                      onClick={onOpenPhpExtensions}
                      title={t.services.phpExtensions}
                      aria-label={t.services.phpExtensions}
                      className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                    >
                      <SlidersHorizontal size={13} style={{ color: 'var(--accent)' }} />
                    </button>
                  )}

                  {svc.id === 'mysql' && (
                    <>
                      <button
                        type="button"
                        onClick={onOpenAdminer}
                        title={t.services.openAdminer}
                        aria-label={t.services.openAdminer}
                        className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                      >
                        <Database size={13} style={{ color: 'var(--accent)' }} />
                      </button>
                      {(bootstrap.config.db_client_path ||
                        bootstrap.inventory.detected_db_clients.length > 0) && (
                        <button
                          type="button"
                          onClick={onLaunchDbClient}
                          title={t.services.launchDbClient}
                          aria-label={t.services.launchDbClient}
                          className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                        >
                          <ExternalLink size={13} />
                        </button>
                      )}
                    </>
                  )}

                  {svc.id === 'mailpit' && isRunning && (
                    <button
                      type="button"
                      onClick={() => onOpenUrl(`http://127.0.0.1:${svc.port}`)}
                      title={t.services.openUi}
                      aria-label={t.services.openUi}
                      className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                    >
                      <ExternalLink size={13} style={{ color: 'var(--amber)' }} />
                    </button>
                  )}

                  {!isRunning ? (
                    <button
                      type="button"
                      disabled={!svc.installed || isBusy}
                      onClick={() => onStart(svc.id)}
                      title={t.services.start}
                      aria-label={t.services.start}
                      className="ui-btn ui-btn-primary w-7 h-7 p-0 flex items-center justify-center shrink-0"
                    >
                      {isBusy ? <Loader2 size={13} className="animate-spin" /> : <Play size={13} />}
                    </button>
                  ) : (
                    <>
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => onRestart(svc.id)}
                        title={t.services.restart}
                        aria-label={t.services.restart}
                        className="ui-btn w-7 h-7 p-0 flex items-center justify-center shrink-0"
                      >
                        <RotateCw size={13} className={isBusy ? 'animate-spin' : ''} />
                      </button>
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => onStop(svc.id)}
                        title={t.services.stop}
                        aria-label={t.services.stop}
                        className="ui-btn ui-btn-danger w-7 h-7 p-0 flex items-center justify-center shrink-0"
                      >
                        {isBusy ? <Loader2 size={13} className="animate-spin" /> : <Square size={13} />}
                      </button>
                    </>
                  )}
                </div>
              </div>

              {svc.last_error && (
                <div
                  className="font-mono text-[11px] px-2.5 py-1.5 rounded border selectable"
                  style={{
                    background: 'var(--red-soft)',
                    borderColor: 'var(--red)',
                    color: 'var(--red)',
                  }}
                >
                  {svc.last_error}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
