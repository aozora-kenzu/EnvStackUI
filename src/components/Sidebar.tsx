import {
  Cpu,
  Database,
  FolderOpen,
  Globe,
  LayoutDashboard,
  Loader2,
  Mail,
  Package,
  Play,
  RefreshCw,
  RotateCw,
  ScrollText,
  Server,
  Settings,
  Square,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { NavTab, ServiceStatus } from '../types/envstack';

interface SidebarProps {
  activeTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  services: ServiceStatus[];
  sitesCount: number;
  mailpitUnread: number;
  busyAll: boolean;
  rescanning: boolean;
  onStartAll: () => void;
  onRestartAll: () => void;
  onStopAll: () => void;
  onOpenAdminer: () => void;
  onOpenMailpit: () => void;
  onOpenWww: () => void;
  onRescan: () => void;
}

export function Sidebar({
  activeTab,
  onSelectTab,
  services,
  sitesCount,
  mailpitUnread,
  busyAll,
  rescanning,
  onStartAll,
  onRestartAll,
  onStopAll,
  onOpenAdminer,
  onOpenMailpit,
  onOpenWww,
  onRescan,
}: SidebarProps) {
  const { t } = useI18n();
  const runningCount = services.filter((s) => s.state === 'running').length;
  const anyInstalled = services.some((s) => s.installed);
  const mailpitRunning = services.some((s) => s.id === 'mailpit' && s.state === 'running');

  const navItems: { id: NavTab; label: string; icon: React.ReactNode; badge?: string }[] = [
    {
      id: 'overview',
      label: t.nav.overview,
      icon: <LayoutDashboard size={14} />,
    },
    {
      id: 'services',
      label: t.nav.services,
      icon: <Server size={14} />,
      badge: runningCount > 0 ? `${runningCount}` : undefined,
    },
    {
      id: 'sites',
      label: t.nav.sites,
      icon: <Globe size={14} />,
      badge: sitesCount > 0 ? `${sitesCount}` : undefined,
    },
    {
      id: 'runtimes',
      label: t.nav.runtimes,
      icon: <Cpu size={14} />,
    },
    {
      id: 'packages',
      label: t.nav.packages,
      icon: <Package size={14} />,
    },
    {
      id: 'logs',
      label: t.nav.logs,
      icon: <ScrollText size={14} />,
    },
    {
      id: 'settings',
      label: t.nav.settings,
      icon: <Settings size={14} />,
    },
  ];

  return (
    <aside
      className="w-[210px] shrink-0 border-r flex flex-col justify-between p-2"
      style={{
        background: 'var(--bg-sidebar)',
        borderColor: 'var(--border)',
      }}
    >
      {/* Navigation */}
      <nav className="flex flex-col gap-0.5">
        {navItems.map((item) => {
          const active = activeTab === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelectTab(item.id)}
              className="ui-icon-btn w-full justify-between px-2.5 py-1.5 text-[12px] font-medium border"
              style={{
                background: active ? 'var(--bg-active)' : 'transparent',
                borderColor: active ? 'var(--border)' : 'transparent',
                color: active ? 'var(--text-primary)' : 'var(--text-muted)',
              }}
            >
              <span className="flex items-center gap-2">
                <span style={{ color: active ? 'var(--accent)' : 'var(--text-dim)' }}>
                  {item.icon}
                </span>
                <span>{item.label}</span>
              </span>
              {item.badge && (
                <span
                  className="font-mono text-[10px] px-1.5 py-0.2 rounded"
                  style={{
                    background:
                      item.id === 'services' && runningCount > 0
                        ? 'var(--green-soft)'
                        : 'var(--bg-elevated)',
                    color:
                      item.id === 'services' && runningCount > 0
                        ? 'var(--green)'
                        : 'var(--text-muted)',
                  }}
                >
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Bottom Quick Actions */}
      <div className="pt-2 border-t flex flex-col gap-1.5" style={{ borderColor: 'var(--border-subtle)' }}>
        <div
          className="px-1.5 text-[10px] uppercase tracking-wider font-semibold"
          style={{ color: 'var(--text-dim)' }}
        >
          {t.sidebar.quickActions}
        </div>

        <div className="grid grid-cols-2 gap-1">
          <button
            type="button"
            disabled={busyAll || !anyInstalled}
            onClick={onStartAll}
            className="ui-btn text-[11px] py-1"
          >
            {busyAll ? (
              <Loader2 size={11} className="animate-spin" />
            ) : (
              <Play size={11} style={{ color: 'var(--green)' }} />
            )}
            <span>{t.sidebar.startAll}</span>
          </button>
          <button
            type="button"
            disabled={busyAll || runningCount === 0}
            onClick={onStopAll}
            className="ui-btn text-[11px] py-1"
          >
            <Square size={11} style={{ color: 'var(--red)' }} />
            <span>{t.sidebar.stopAll}</span>
          </button>
        </div>

        <button
          type="button"
          disabled={busyAll || runningCount === 0}
          onClick={onRestartAll}
          className="ui-btn w-full text-[11px] py-1"
        >
          <RotateCw size={11} className={busyAll ? 'animate-spin' : ''} style={{ color: 'var(--amber)' }} />
          <span>{t.sidebar.restartAll}</span>
        </button>

        <div className="flex flex-col gap-0.5">
          <button
            type="button"
            onClick={onOpenAdminer}
            className="ui-icon-btn w-full px-2 py-1 text-[11px]"
            style={{ color: 'var(--text-muted)' }}
          >
            <Database size={12} style={{ color: 'var(--accent)' }} />
            <span>{t.sidebar.database}</span>
          </button>

          <button
            type="button"
            disabled={!mailpitRunning}
            onClick={onOpenMailpit}
            className="ui-icon-btn w-full justify-between px-2 py-1 text-[11px]"
            style={{ color: 'var(--text-muted)' }}
          >
            <span className="flex items-center gap-2">
              <Mail size={12} style={{ color: 'var(--amber)' }} />
              <span>{t.sidebar.mailpit}</span>
            </span>
            {mailpitUnread > 0 && (
              <span
                className="font-mono text-[10px] px-1.5 rounded"
                style={{ background: 'var(--amber-soft)', color: 'var(--amber)' }}
              >
                {mailpitUnread}
              </span>
            )}
          </button>

          <button
            type="button"
            onClick={onOpenWww}
            className="ui-icon-btn w-full px-2 py-1 text-[11px]"
            style={{ color: 'var(--text-muted)' }}
          >
            <FolderOpen size={12} />
            <span>{t.sidebar.openRoot}</span>
          </button>

          <button
            type="button"
            disabled={rescanning}
            onClick={onRescan}
            className="ui-icon-btn w-full px-2 py-1 text-[11px]"
            style={{ color: 'var(--text-muted)' }}
          >
            <RefreshCw size={12} className={rescanning ? 'animate-spin' : ''} />
            <span>{t.sidebar.rescan}</span>
          </button>
        </div>
      </div>
    </aside>
  );
}
