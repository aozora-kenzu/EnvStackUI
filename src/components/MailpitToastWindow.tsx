import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ExternalLink, Mail, X } from 'lucide-react';
import { api } from '../lib/api';
import type { MailpitNotification } from '../types/envstack';

export function MailpitToastWindow() {
  const [notif, setNotif] = useState<MailpitNotification | null>(null);
  const timerRef = useRef<number | null>(null);
  const hoveredRef = useRef(false);

  const scheduleHide = () => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
    }
    timerRef.current = window.setTimeout(() => {
      if (!hoveredRef.current) {
        void api.hideMailpitToast();
      }
    }, 8000);
  };

  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => e.preventDefault();
    window.addEventListener('contextmenu', onContextMenu);

    void api.getLatestMailpitNotification().then((data) => {
      if (data) {
        setNotif(data);
        document.documentElement.setAttribute(
          'data-theme',
          data.theme === 'light' ? 'light' : 'dark',
        );
        scheduleHide();
      }
    });

    const unlistenPromise = listen<MailpitNotification>('mailpit-toast-update', (ev) => {
      setNotif(ev.payload);
      document.documentElement.setAttribute(
        'data-theme',
        ev.payload.theme === 'light' ? 'light' : 'dark',
      );
      scheduleHide();
    });

    return () => {
      window.removeEventListener('contextmenu', onContextMenu);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      void unlistenPromise.then((fn) => fn());
    };
  }, []);

  if (!notif) return null;

  const isId = notif.language === 'id';
  const sender = notif.from_name
    ? `${notif.from_name} <${notif.from_address}>`
    : notif.from_address || 'Mailpit';
  const subject = notif.subject || (isId ? '(Tanpa Subjek)' : '(No Subject)');

  const handleOpenMailpit = async () => {
    await api.openExternalUrl(notif.url);
    await api.hideMailpitToast();
  };

  return (
    <div
      onMouseEnter={() => {
        hoveredRef.current = true;
        if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      }}
      onMouseLeave={() => {
        hoveredRef.current = false;
        scheduleHide();
      }}
      onClick={() => void handleOpenMailpit()}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') void handleOpenMailpit();
      }}
      className="h-screen w-screen p-2.5 flex flex-col justify-between cursor-pointer border select-none transition-colors"
      style={{
        background: 'var(--bg-elevated)',
        borderColor: 'var(--border)',
        borderLeftWidth: '3px',
        borderLeftColor: 'var(--amber)',
        color: 'var(--text-primary)',
      }}
    >
      {/* Top Row */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0">
          <Mail size={13} style={{ color: 'var(--amber)' }} className="shrink-0" />
          <span className="text-[11px] font-semibold truncate">
            {isId ? 'Email Baru · Mailpit' : 'New Email · Mailpit'}
          </span>
          {notif.unread > 0 && (
            <span
              className="font-mono text-[10px] px-1.5 rounded shrink-0"
              style={{ background: 'var(--amber-soft)', color: 'var(--amber)' }}
            >
              {notif.unread}
            </span>
          )}
        </div>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            void api.hideMailpitToast();
          }}
          className="p-0.5 rounded cursor-pointer hover:bg-[var(--bg-hover)] shrink-0"
          style={{ color: 'var(--text-dim)' }}
          aria-label="Close"
        >
          <X size={13} />
        </button>
      </div>

      {/* Sender + Subject */}
      <div className="min-w-0 flex flex-col gap-0.5">
        <div className="text-[11px] font-mono truncate" style={{ color: 'var(--text-muted)' }}>
          {sender}
        </div>
        <div className="text-[12px] font-semibold truncate">{subject}</div>
      </div>

      {/* Footer Hint */}
      <div className="flex items-center justify-between gap-2 text-[10px]" style={{ color: 'var(--accent)' }}>
        <span className="truncate font-mono" style={{ color: 'var(--text-dim)' }}>
          {notif.snippet || notif.url}
        </span>
        <span className="flex items-center gap-1 shrink-0 font-medium">
          <span>{isId ? 'Buka Mailpit' : 'Open Mailpit'}</span>
          <ExternalLink size={10} />
        </span>
      </div>
    </div>
  );
}
