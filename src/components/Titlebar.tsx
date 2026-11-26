import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Info,
  Languages,
  Minus,
  Moon,
  Search,
  Square,
  Sun,
  Terminal,
  X,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { LanguageCode, ThemeMode } from '../types/envstack';

interface TitlebarProps {
  theme: ThemeMode;
  lang: LanguageCode;
  hasUpdateDot: boolean;
  onToggleTheme: () => void;
  onToggleLang: () => void;
  onOpenPalette: () => void;
  onOpenTerminal: () => void;
  onOpenAbout: () => void;
}

export function Titlebar({
  theme,
  lang,
  hasUpdateDot,
  onToggleTheme,
  onToggleLang,
  onOpenPalette,
  onOpenTerminal,
  onOpenAbout,
}: TitlebarProps) {
  const { t } = useI18n();
  const appWindow = getCurrentWindow();

  return (
    <header
      data-tauri-drag-region
      className="h-9 shrink-0 flex items-center justify-between px-2 border-b select-none"
      style={{
        background: 'var(--bg-sidebar)',
        borderColor: 'var(--border)',
      }}
    >
      {/* Left: Brand mark */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 pl-1.5 pr-3 pointer-events-none"
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <rect
            x="2"
            y="2"
            width="20"
            height="20"
            rx="4"
            fill="var(--bg-surface)"
            stroke="var(--border)"
            strokeWidth="1.5"
          />
          <rect x="5.5" y="6.5" width="11" height="2.4" rx="1" fill="var(--accent)" />
          <rect x="7.5" y="10.8" width="11" height="2.4" rx="1" fill="var(--green)" />
          <rect x="5.5" y="15.1" width="11" height="2.4" rx="1" fill="var(--accent)" />
        </svg>
        <span className="font-semibold text-[12px] tracking-tight" style={{ color: 'var(--text-primary)' }}>
          {t.appName}
        </span>
      </div>

      {/* Center: Command Palette trigger */}
      <button
        type="button"
        onClick={onOpenPalette}
        className="ui-icon-btn flex items-center justify-between gap-4 h-6 px-2.5 rounded border text-[11px] min-w-[260px] max-w-[380px] flex-1 mx-4"
        style={{
          background: 'var(--bg-input)',
          borderColor: 'var(--border)',
          color: 'var(--text-muted)',
        }}
      >
        <span className="flex items-center gap-1.5 truncate">
          <Search size={12} style={{ color: 'var(--text-dim)' }} />
          <span className="truncate">{t.titlebar.commandPaletteHint}</span>
        </span>
        <kbd
          className="font-mono text-[10px] px-1.5 py-0.2 rounded border shrink-0"
          style={{
            background: 'var(--bg-surface)',
            borderColor: 'var(--border)',
            color: 'var(--text-dim)',
          }}
        >
          Ctrl+K
        </kbd>
      </button>

      {/* Right: Quick Toggles + Window Controls */}
      <div className="flex items-center gap-0.5">
        <button
          type="button"
          onClick={onOpenTerminal}
          title={t.titlebar.terminal}
          className="ui-icon-btn h-7 px-2 text-[11px]"
          style={{ color: 'var(--text-muted)' }}
        >
          <Terminal size={13} />
          <span className="hidden sm:inline">{t.titlebar.terminal}</span>
        </button>

        <button
          type="button"
          onClick={onToggleLang}
          title={t.titlebar.switchLang}
          className="ui-icon-btn h-7 px-2 text-[11px] font-mono"
          style={{ color: 'var(--text-muted)' }}
        >
          <Languages size={13} />
          <span>{lang.toUpperCase()}</span>
        </button>

        <button
          type="button"
          onClick={onToggleTheme}
          title={t.titlebar.switchTheme}
          className="ui-icon-btn h-7 w-7 justify-center"
          style={{ color: 'var(--text-muted)' }}
        >
          {theme === 'dark' ? <Sun size={13} /> : <Moon size={13} />}
        </button>

        <button
          type="button"
          onClick={onOpenAbout}
          title={t.titlebar.about}
          className="ui-icon-btn relative h-7 px-2 text-[11px]"
          style={{ color: 'var(--text-muted)' }}
        >
          <Info size={13} />
          <span className="hidden md:inline">{t.titlebar.about}</span>
          {hasUpdateDot && (
            <span
              className="absolute top-1.5 right-1.5 w-1.5 h-1.5 rounded-full"
              style={{ background: 'var(--amber)' }}
            />
          )}
        </button>

        <div className="w-px h-4 mx-1" style={{ background: 'var(--border)' }} />

        <button
          type="button"
          onClick={() => appWindow.minimize()}
          title={t.titlebar.minimize}
          className="ui-icon-btn h-7 w-8 justify-center"
          style={{ color: 'var(--text-muted)' }}
        >
          <Minus size={13} />
        </button>

        <button
          type="button"
          onClick={() => appWindow.toggleMaximize()}
          title={t.titlebar.maximize}
          className="ui-icon-btn h-7 w-8 justify-center"
          style={{ color: 'var(--text-muted)' }}
        >
          <Square size={11} />
        </button>

        <button
          type="button"
          onClick={() => appWindow.close()}
          title={t.titlebar.close}
          className="ui-icon-btn h-7 w-8 justify-center hover:!bg-[var(--red)] hover:!text-white"
          style={{ color: 'var(--text-muted)' }}
        >
          <X size={13} />
        </button>
      </div>
    </header>
  );
}
