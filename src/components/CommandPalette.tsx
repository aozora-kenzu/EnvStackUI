import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Cpu,
  Database,
  Globe,
  Info,
  Languages,
  LayoutDashboard,
  Moon,
  Package,
  Play,
  RefreshCw,
  RotateCw,
  ScrollText,
  Search,
  Server,
  Settings,
  ShieldCheck,
  Square,
  Terminal,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap, NavTab } from '../types/envstack';

export interface PaletteCommand {
  id: string;
  category: string;
  label: string;
  hint?: string;
  icon: React.ReactNode;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  bootstrap: AppBootstrap;
  onClose: () => void;
  onSelectTab: (tab: NavTab) => void;
  onStartAll: () => void;
  onRestartAll: () => void;
  onStopAll: () => void;
  onOpenTerminal: () => void;
  onOpenAdminer: () => void;
  onSyncHosts: () => void;
  onTrustCa: () => void;
  onRescan: () => void;
  onToggleTheme: () => void;
  onToggleLang: () => void;
  onOpenAbout: () => void;
  onOpenSiteUrl: (url: string) => void;
}

export function CommandPalette({
  open,
  bootstrap,
  onClose,
  onSelectTab,
  onStartAll,
  onRestartAll,
  onStopAll,
  onOpenTerminal,
  onOpenAdminer,
  onSyncHosts,
  onTrustCa,
  onRescan,
  onToggleTheme,
  onToggleLang,
  onOpenAbout,
  onOpenSiteUrl,
}: CommandPaletteProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState('');
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery('');
      setActiveIdx(0);
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  }, [open]);

  const commands = useMemo<PaletteCommand[]>(() => {
    const base: PaletteCommand[] = [
      {
        id: 'nav-overview',
        category: t.palette.catNavigation,
        label: t.nav.overview,
        icon: <LayoutDashboard size={14} />,
        run: () => onSelectTab('overview'),
      },
      {
        id: 'nav-services',
        category: t.palette.catNavigation,
        label: t.nav.services,
        icon: <Server size={14} />,
        run: () => onSelectTab('services'),
      },
      {
        id: 'nav-sites',
        category: t.palette.catNavigation,
        label: t.nav.sites,
        icon: <Globe size={14} />,
        run: () => onSelectTab('sites'),
      },
      {
        id: 'nav-runtimes',
        category: t.palette.catNavigation,
        label: t.nav.runtimes,
        icon: <Cpu size={14} />,
        run: () => onSelectTab('runtimes'),
      },
      {
        id: 'nav-packages',
        category: t.palette.catNavigation,
        label: t.nav.packages,
        icon: <Package size={14} />,
        run: () => onSelectTab('packages'),
      },
      {
        id: 'nav-logs',
        category: t.palette.catNavigation,
        label: t.nav.logs,
        icon: <ScrollText size={14} />,
        run: () => onSelectTab('logs'),
      },
      {
        id: 'nav-settings',
        category: t.palette.catNavigation,
        label: t.nav.settings,
        icon: <Settings size={14} />,
        run: () => onSelectTab('settings'),
      },
      {
        id: 'act-start-all',
        category: t.palette.catServices,
        label: t.palette.cmdStartAll,
        icon: <Play size={14} style={{ color: 'var(--green)' }} />,
        run: onStartAll,
      },
      {
        id: 'act-restart-all',
        category: t.palette.catServices,
        label: t.palette.cmdRestartAll,
        icon: <RotateCw size={14} style={{ color: 'var(--amber)' }} />,
        run: onRestartAll,
      },
      {
        id: 'act-stop-all',
        category: t.palette.catServices,
        label: t.palette.cmdStopAll,
        icon: <Square size={14} style={{ color: 'var(--red)' }} />,
        run: onStopAll,
      },
      {
        id: 'act-terminal',
        category: t.palette.catActions,
        label: t.palette.cmdTerminal,
        icon: <Terminal size={14} />,
        run: onOpenTerminal,
      },
      {
        id: 'act-adminer',
        category: t.palette.catActions,
        label: t.palette.cmdAdminer,
        icon: <Database size={14} />,
        run: onOpenAdminer,
      },
      {
        id: 'act-sync-hosts',
        category: t.palette.catActions,
        label: t.palette.cmdSyncHosts,
        icon: <Globe size={14} />,
        run: onSyncHosts,
      },
      {
        id: 'act-trust-ca',
        category: t.palette.catActions,
        label: t.palette.cmdTrustCa,
        icon: <ShieldCheck size={14} />,
        run: onTrustCa,
      },
      {
        id: 'act-rescan',
        category: t.palette.catActions,
        label: t.palette.cmdRescan,
        icon: <RefreshCw size={14} />,
        run: onRescan,
      },
      {
        id: 'act-theme',
        category: t.palette.catActions,
        label: t.palette.cmdToggleTheme,
        icon: <Moon size={14} />,
        run: onToggleTheme,
      },
      {
        id: 'act-lang',
        category: t.palette.catActions,
        label: t.palette.cmdToggleLang,
        icon: <Languages size={14} />,
        run: onToggleLang,
      },
      {
        id: 'act-about',
        category: t.palette.catActions,
        label: t.palette.cmdAbout,
        icon: <Info size={14} />,
        run: onOpenAbout,
      },
    ];

    for (const site of bootstrap.sites) {
      base.push({
        id: `site-${site.domain}`,
        category: t.palette.catSites,
        label: site.domain,
        hint: site.framework,
        icon: <Globe size={14} style={{ color: 'var(--accent)' }} />,
        run: () => onOpenSiteUrl(site.has_ssl && bootstrap.config.auto_ssl ? site.https_url : site.http_url),
      });
    }

    return base;
  }, [
    t,
    bootstrap.sites,
    bootstrap.config.auto_ssl,
    onSelectTab,
    onStartAll,
    onRestartAll,
    onStopAll,
    onOpenTerminal,
    onOpenAdminer,
    onSyncHosts,
    onTrustCa,
    onRescan,
    onToggleTheme,
    onToggleLang,
    onOpenAbout,
    onOpenSiteUrl,
  ]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.label.toLowerCase().includes(q) ||
        c.category.toLowerCase().includes(q) ||
        (c.hint?.toLowerCase().includes(q) ?? false),
    );
  }, [commands, query]);

  useEffect(() => {
    setActiveIdx(0);
  }, [query]);

  if (!open) return null;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIdx((i) => (filtered.length === 0 ? 0 : (i + 1) % filtered.length));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIdx((i) =>
        filtered.length === 0 ? 0 : (i - 1 + filtered.length) % filtered.length,
      );
    } else if (e.key === 'Enter' && filtered[activeIdx]) {
      e.preventDefault();
      filtered[activeIdx].run();
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 bg-black/45"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[560px] rounded-md border overflow-hidden"
        style={{
          background: 'var(--bg-surface)',
          borderColor: 'var(--border)',
        }}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <div
          className="flex items-center gap-2 px-3 py-2.5 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <Search size={14} style={{ color: 'var(--accent)' }} />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t.palette.placeholder}
            className="w-full bg-transparent outline-none text-[13px]"
            style={{ color: 'var(--text-primary)' }}
          />
          <kbd
            className="font-mono text-[10px] px-1.5 py-0.5 rounded border"
            style={{
              background: 'var(--bg-elevated)',
              borderColor: 'var(--border)',
              color: 'var(--text-dim)',
            }}
          >
            ESC
          </kbd>
        </div>

        <div className="max-h-[340px] overflow-y-auto p-1.5">
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-[12px]" style={{ color: 'var(--text-dim)' }}>
              {t.palette.noResults}
            </div>
          ) : (
            filtered.map((cmd, idx) => {
              const selected = idx === activeIdx;
              return (
                <button
                  key={cmd.id}
                  type="button"
                  onMouseEnter={() => setActiveIdx(idx)}
                  onClick={() => {
                    cmd.run();
                    onClose();
                  }}
                  className="w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left cursor-pointer border"
                  style={{
                    background: selected ? 'var(--bg-active)' : 'transparent',
                    borderColor: selected ? 'var(--border)' : 'transparent',
                  }}
                >
                  <span className="flex items-center gap-2.5 min-w-0">
                    <span style={{ color: 'var(--text-muted)' }}>{cmd.icon}</span>
                    <span className="text-[12px] truncate" style={{ color: 'var(--text-primary)' }}>
                      {cmd.label}
                    </span>
                  </span>
                  <span className="flex items-center gap-2 shrink-0">
                    {cmd.hint && (
                      <span className="font-mono text-[10px]" style={{ color: 'var(--text-dim)' }}>
                        {cmd.hint}
                      </span>
                    )}
                    <span
                      className="text-[10px] px-1.5 py-0.5 rounded"
                      style={{
                        background: 'var(--bg-elevated)',
                        color: 'var(--text-muted)',
                      }}
                    >
                      {cmd.category}
                    </span>
                  </span>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
