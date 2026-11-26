import {
  ArrowUpRight,
  Cpu,
  Database,
  FolderOpen,
  Globe,
  Loader2,
  Mail,
  Play,
  RefreshCw,
  RotateCw,
  Server,
  ShieldAlert,
  ShieldCheck,
  Square,
  Terminal,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap, NavTab, ServiceStatus } from '../types/envstack';

interface OverviewViewProps {
  bootstrap: AppBootstrap;
  busyService: string | null;
  busyAll: boolean;
  rescanning: boolean;
  onSelectTab: (tab: NavTab) => void;
  onStartService: (id: string) => void;
  onStopService: (id: string) => void;
  onStartAll: () => void;
  onRestartAll: () => void;
  onStopAll: () => void;
  onOpenTerminal: () => void;
  onRescan: () => void;
  onOpenAdminer: () => void;
  onOpenMailpit: () => void;
  onOpenWww: () => void;
  onOpenUrl: (url: string) => void;
}

const RUNTIME_SUMMARY = [
  { id: 'php', label: 'PHP' },
  { id: 'node', label: 'Node.js' },
  { id: 'composer', label: 'Composer' },
  { id: 'git', label: 'Git' },
  { id: 'python', label: 'Python' },
  { id: 'go', label: 'Go' },
];

export function OverviewView({
  bootstrap,
  busyService,
  busyAll,
  rescanning,
  onSelectTab,
  onStartService,
  onStopService,
  onStartAll,
  onRestartAll,
  onStopAll,
  onOpenTerminal,
  onRescan,
  onOpenAdminer,
  onOpenMailpit,
  onOpenWww,
  onOpenUrl,
}: OverviewViewProps) {
  const { t, format } = useI18n();

  const runningCount = bootstrap.services.filter((s) => s.state === 'running').length;
  const totalServices = bootstrap.services.length;
  const anyInstalled = bootstrap.services.some((s) => s.installed);
  const mailpitRunning = bootstrap.services.some(
    (s) => s.id === 'mailpit' && s.state === 'running',
  );

  const activePhp = bootstrap.inventory.binaries.php?.find(
    (b) => b.id === bootstrap.config.active_versions.php,
  );
  const activeNode = bootstrap.inventory.binaries.node?.find(
    (b) => b.id === bootstrap.config.active_versions.node,
  );

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

  return (
    <div className="flex flex-col gap-4">
      {/* Header + Toolbar */}
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.overview.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.overview.subtitle}
          </p>
        </div>
        <div className="flex items-center gap-1.5 flex-wrap">
          <button
            type="button"
            disabled={busyAll || !anyInstalled}
            onClick={onStartAll}
            className="ui-btn ui-btn-primary text-[11px]"
          >
            {busyAll ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <Play size={12} />
            )}
            <span>{t.sidebar.startAll}</span>
          </button>
          <button
            type="button"
            disabled={busyAll || runningCount === 0}
            onClick={onRestartAll}
            className="ui-btn text-[11px]"
          >
            <RotateCw size={12} className={busyAll ? 'animate-spin' : ''} style={{ color: 'var(--amber)' }} />
            <span>{t.sidebar.restartAll}</span>
          </button>
          <button
            type="button"
            disabled={busyAll || runningCount === 0}
            onClick={onStopAll}
            className="ui-btn ui-btn-danger text-[11px]"
          >
            <Square size={12} />
            <span>{t.sidebar.stopAll}</span>
          </button>
          <button type="button" onClick={onOpenTerminal} className="ui-btn text-[11px]">
            <Terminal size={12} />
            <span>{t.titlebar.terminal}</span>
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

      {/* 4 KPI Cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-2.5">
        <button
          type="button"
          onClick={() => onSelectTab('services')}
          className="ui-panel p-3 text-left cursor-pointer hover:border-[var(--accent)] transition-colors"
        >
          <div className="flex items-center justify-between text-[11px]" style={{ color: 'var(--text-muted)' }}>
            <span>{t.overview.cardServices}</span>
            <Server size={13} style={{ color: runningCount > 0 ? 'var(--green)' : 'var(--text-dim)' }} />
          </div>
          <div className="font-mono text-[15px] font-semibold mt-1">
            {runningCount} / {totalServices}
          </div>
          <div className="text-[11px] mt-0.5" style={{ color: 'var(--text-dim)' }}>
            {format(t.overview.runningSummary, { running: runningCount, total: totalServices })}
          </div>
        </button>

        <button
          type="button"
          onClick={() => onSelectTab('sites')}
          className="ui-panel p-3 text-left cursor-pointer hover:border-[var(--accent)] transition-colors"
        >
          <div className="flex items-center justify-between text-[11px]" style={{ color: 'var(--text-muted)' }}>
            <span>{t.overview.cardSites}</span>
            <Globe size={13} style={{ color: 'var(--accent)' }} />
          </div>
          <div className="font-mono text-[15px] font-semibold mt-1">
            {format(t.overview.sitesCount, { count: bootstrap.sites.length })}
          </div>
          <div
            className="text-[11px] font-mono mt-0.5"
            style={{ color: bootstrap.hosts_status.in_sync ? 'var(--green)' : 'var(--amber)' }}
          >
            *.{bootstrap.config.tld} ·{' '}
            {bootstrap.hosts_status.in_sync ? t.statusBar.hostsSynced : t.statusBar.hostsPending}
          </div>
        </button>

        <button
          type="button"
          onClick={() => onSelectTab('sites')}
          className="ui-panel p-3 text-left cursor-pointer hover:border-[var(--accent)] transition-colors"
        >
          <div className="flex items-center justify-between text-[11px]" style={{ color: 'var(--text-muted)' }}>
            <span>{t.overview.cardSsl}</span>
            {bootstrap.ssl_status.ca_trusted ? (
              <ShieldCheck size={13} style={{ color: 'var(--green)' }} />
            ) : (
              <ShieldAlert size={13} style={{ color: 'var(--amber)' }} />
            )}
          </div>
          <div
            className="text-[13px] font-semibold mt-1"
            style={{
              color: bootstrap.ssl_status.ca_trusted ? 'var(--green)' : 'var(--amber)',
            }}
          >
            {bootstrap.ssl_status.ca_trusted ? t.statusBar.caTrusted : t.statusBar.caUntrusted}
          </div>
          <div className="text-[11px] font-mono mt-0.5" style={{ color: 'var(--text-dim)' }}>
            {format(t.overview.certsReady, { count: bootstrap.ssl_status.cert_count })}
          </div>
        </button>

        <button
          type="button"
          onClick={() => onSelectTab('runtimes')}
          className="ui-panel p-3 text-left cursor-pointer hover:border-[var(--accent)] transition-colors"
        >
          <div className="flex items-center justify-between text-[11px]" style={{ color: 'var(--text-muted)' }}>
            <span>{t.overview.cardRuntimes}</span>
            <Cpu size={13} style={{ color: 'var(--accent)' }} />
          </div>
          <div className="font-mono text-[13px] font-semibold mt-1 truncate">
            {activePhp ? `PHP ${activePhp.version}` : t.overview.noRuntime}
          </div>
          <div className="text-[11px] font-mono mt-0.5 truncate" style={{ color: 'var(--text-dim)' }}>
            {activeNode ? `Node ${activeNode.version}` : `PATH: ${bootstrap.config.user_path_entries.length}`}
          </div>
        </button>
      </div>

      {/* 2-Column Workspace */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
        {/* Left: Quick Service Control */}
        <div className="ui-panel p-3.5 flex flex-col gap-2.5">
          <div className="flex items-center justify-between">
            <span className="text-[13px] font-semibold">{t.overview.servicesPanelTitle}</span>
            <button
              type="button"
              onClick={() => onSelectTab('services')}
              className="text-[11px] flex items-center gap-0.5 cursor-pointer hover:underline"
              style={{ color: 'var(--accent)' }}
            >
              <span>{t.overview.servicesPanelViewAll}</span>
              <ArrowUpRight size={11} />
            </button>
          </div>

          <div className="flex flex-col gap-1.5">
            {bootstrap.services.map((svc) => {
              const running = svc.state === 'running';
              const busy = busyService === svc.id;
              return (
                <div
                  key={svc.id}
                  className="flex items-center justify-between px-2.5 py-2 rounded border"
                  style={{
                    background: 'var(--bg-elevated)',
                    borderColor: running ? 'var(--green)' : 'var(--border)',
                  }}
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <span
                      className="w-2 h-2 rounded-full shrink-0"
                      style={{ background: stateColor(svc.state) }}
                    />
                    <div className="min-w-0">
                      <div className="text-[12px] font-semibold truncate">{svc.label}</div>
                      <div className="text-[10px] font-mono truncate" style={{ color: 'var(--text-dim)' }}>
                        {svc.version ? `v${svc.version}` : t.services.notInstalled} · :{svc.port}
                      </div>
                    </div>
                  </div>

                  {!running ? (
                    <button
                      type="button"
                      disabled={!svc.installed || busy}
                      onClick={() => onStartService(svc.id)}
                      className="ui-btn ui-btn-primary text-[11px] py-1 px-2.5 shrink-0"
                    >
                      {busy ? <Loader2 size={11} className="animate-spin" /> : <Play size={11} />}
                      <span>{t.services.start}</span>
                    </button>
                  ) : (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onStopService(svc.id)}
                      className="ui-btn ui-btn-danger text-[11px] py-1 px-2.5 shrink-0"
                    >
                      {busy ? <Loader2 size={11} className="animate-spin" /> : <Square size={11} />}
                      <span>{t.services.stop}</span>
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Right: Environment & Toolchains */}
        <div className="ui-panel p-3.5 flex flex-col justify-between gap-3">
          <div className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between">
              <span className="text-[13px] font-semibold">{t.overview.envPanelTitle}</span>
              <button
                type="button"
                onClick={() => onSelectTab('runtimes')}
                className="text-[11px] flex items-center gap-0.5 cursor-pointer hover:underline"
                style={{ color: 'var(--accent)' }}
              >
                <span>{t.overview.envPanelManage}</span>
                <ArrowUpRight size={11} />
              </button>
            </div>

            <div className="grid grid-cols-2 gap-1.5">
              {RUNTIME_SUMMARY.map((rt) => {
                const activeId = bootstrap.config.active_versions[rt.id];
                const item = (bootstrap.inventory.binaries[rt.id] ?? []).find(
                  (b) => b.id === activeId,
                );
                return (
                  <div
                    key={rt.id}
                    className="px-2.5 py-1.5 rounded border flex items-center justify-between gap-2"
                    style={{ background: 'var(--bg-elevated)', borderColor: 'var(--border)' }}
                  >
                    <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                      {rt.label}
                    </span>
                    <span
                      className="font-mono text-[11px] truncate"
                      style={{ color: item ? 'var(--text-primary)' : 'var(--text-dim)' }}
                    >
                      {item ? item.version : '—'}
                    </span>
                  </div>
                );
              })}
            </div>

            <div
              className="rounded border p-2.5 flex flex-col gap-1.5 text-[11px]"
              style={{ background: 'var(--bg-input)', borderColor: 'var(--border)' }}
            >
              <div className="flex items-center justify-between gap-3">
                <span style={{ color: 'var(--text-muted)' }}>{t.overview.appRoot}</span>
                <span className="font-mono truncate selectable" title={bootstrap.root_dir}>
                  {bootstrap.root_dir}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span style={{ color: 'var(--text-muted)' }}>{t.overview.docRoot}</span>
                <span className="font-mono truncate selectable" title={bootstrap.www_root}>
                  {bootstrap.www_root}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span style={{ color: 'var(--text-muted)' }}>{t.overview.linkedFolders}</span>
                <span className="font-mono" style={{ color: 'var(--accent)' }}>
                  {bootstrap.config.external_bin_roots.length}
                </span>
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-1.5">
            <button type="button" onClick={onOpenAdminer} className="ui-btn text-[11px]">
              <Database size={12} style={{ color: 'var(--accent)' }} />
              <span>{t.sidebar.database}</span>
            </button>
            <button
              type="button"
              disabled={!mailpitRunning}
              onClick={onOpenMailpit}
              className="ui-btn text-[11px]"
            >
              <Mail size={12} style={{ color: 'var(--amber)' }} />
              <span>{t.sidebar.mailpit}</span>
            </button>
            <button type="button" onClick={onOpenWww} className="ui-btn text-[11px]">
              <FolderOpen size={12} />
              <span>{t.sidebar.openRoot}</span>
            </button>
          </div>
        </div>
      </div>

      {/* Detected Sites Quick Strip */}
      <div className="ui-panel p-3.5 flex flex-col gap-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[13px] font-semibold">{t.overview.sitesPanelTitle}</span>
          <button
            type="button"
            onClick={() => onSelectTab('sites')}
            className="text-[11px] flex items-center gap-0.5 cursor-pointer hover:underline"
            style={{ color: 'var(--accent)' }}
          >
            <span>{t.overview.sitesPanelViewAll}</span>
            <ArrowUpRight size={11} />
          </button>
        </div>

        {bootstrap.sites.length === 0 ? (
          <div className="text-[12px]" style={{ color: 'var(--text-dim)' }}>
            {t.overview.noSites}
          </div>
        ) : (
          <div className="flex flex-wrap gap-2">
            {bootstrap.sites.slice(0, 8).map((site) => (
              <button
                key={site.domain}
                type="button"
                onClick={() =>
                  onOpenUrl(
                    site.has_ssl && bootstrap.config.auto_ssl
                      ? site.https_url
                      : site.http_url,
                  )
                }
                className="ui-btn text-[11px] font-mono"
              >
                <Globe size={11} style={{ color: 'var(--accent)' }} />
                <span>{site.domain}</span>
                <span
                  className="text-[10px] px-1 rounded"
                  style={{ background: 'var(--bg-surface)', color: 'var(--text-dim)' }}
                >
                  {site.framework}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
