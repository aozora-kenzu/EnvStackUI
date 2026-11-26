import { Mail, ShieldAlert, ShieldCheck } from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap } from '../types/envstack';

interface StatusBarProps {
  bootstrap: AppBootstrap;
  mailpitUnread: number;
  onOpenAbout: () => void;
  onGoSites: () => void;
  onOpenMailpit: () => void;
}

export function StatusBar({
  bootstrap,
  mailpitUnread,
  onOpenAbout,
  onGoSites,
  onOpenMailpit,
}: StatusBarProps) {
  const { t, format } = useI18n();
  const running = bootstrap.services.filter((s) => s.state === 'running').length;
  const total = bootstrap.services.length;

  const activePhp = bootstrap.inventory.binaries.php?.find(
    (b) => b.id === bootstrap.config.active_versions.php,
  );
  const activeNode = bootstrap.inventory.binaries.node?.find(
    (b) => b.id === bootstrap.config.active_versions.node,
  );

  return (
    <footer
      className="h-6 shrink-0 border-t px-3 flex items-center justify-between text-[11px] select-none"
      style={{
        background: 'var(--bg-sidebar)',
        borderColor: 'var(--border)',
        color: 'var(--text-muted)',
      }}
    >
      {/* Left metrics */}
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-1.5 font-mono">
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={{ background: running > 0 ? 'var(--green)' : 'var(--text-dim)' }}
          />
          <span>{format(t.statusBar.runningOf, { running, total })}</span>
        </span>

        {activePhp && (
          <span className="font-mono" style={{ color: 'var(--text-dim)' }}>
            PHP {activePhp.version}
          </span>
        )}

        {activeNode && (
          <span className="font-mono" style={{ color: 'var(--text-dim)' }}>
            Node {activeNode.version}
          </span>
        )}

        <span className="font-mono" style={{ color: 'var(--text-dim)' }}>
          *.{bootstrap.config.tld}
        </span>
      </div>

      {/* Right status pills */}
      <div className="flex items-center gap-3">
        {mailpitUnread > 0 && (
          <button
            type="button"
            onClick={onOpenMailpit}
            className="flex items-center gap-1 font-mono cursor-pointer hover:underline"
            style={{ color: 'var(--amber)' }}
          >
            <Mail size={11} />
            <span>Mailpit ({mailpitUnread})</span>
          </button>
        )}

        <button
          type="button"
          onClick={onGoSites}
          className="flex items-center gap-1 cursor-pointer hover:underline"
          style={{
            color: bootstrap.ssl_status.ca_trusted ? 'var(--green)' : 'var(--amber)',
          }}
        >
          {bootstrap.ssl_status.ca_trusted ? (
            <ShieldCheck size={11} />
          ) : (
            <ShieldAlert size={11} />
          )}
          <span>
            {bootstrap.ssl_status.ca_trusted
              ? t.statusBar.caTrusted
              : t.statusBar.caUntrusted}
          </span>
        </button>

        {!bootstrap.hosts_status.in_sync && (
          <button
            type="button"
            onClick={onGoSites}
            className="font-mono cursor-pointer hover:underline"
            style={{ color: 'var(--amber)' }}
          >
            {t.statusBar.hostsPending}
          </button>
        )}

        <span className="font-mono" style={{ color: 'var(--text-dim)' }}>
          {t.statusBar.portable}
        </span>

        <button
          type="button"
          onClick={onOpenAbout}
          className="font-mono cursor-pointer hover:text-[var(--accent)]"
          style={{ color: 'var(--text-muted)' }}
        >
          v{bootstrap.version}
        </button>
      </div>
    </footer>
  );
}
