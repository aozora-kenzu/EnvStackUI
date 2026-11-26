import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  Clipboard,
  Copy,
  FolderOpen,
  Info,
  Languages,
  Moon,
  Play,
  RefreshCw,
  RotateCw,
  Scissors,
  Search,
  Square,
  Sun,
  Terminal,
} from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap } from '../types/envstack';

interface ContextMenuProps {
  bootstrap: AppBootstrap;
  onOpenPalette: () => void;
  onStartAll: () => void;
  onRestartAll: () => void;
  onStopAll: () => void;
  onOpenTerminal: () => void;
  onOpenWww: () => void;
  onRescan: () => void;
  onToggleTheme: () => void;
  onToggleLang: () => void;
  onOpenAbout: () => void;
  onNotify: (msg: string, kind?: 'ok' | 'err' | 'info') => void;
}

type EditableElement = HTMLInputElement | HTMLTextAreaElement;

interface MenuState {
  x: number;
  y: number;
  selectedText: string;
  editableEl: EditableElement | null;
}

function resolveEditable(target: EventTarget | null): EditableElement | null {
  if (!(target instanceof HTMLElement)) return null;
  if (target instanceof HTMLTextAreaElement) {
    return target.readOnly || target.disabled ? null : target;
  }
  if (target instanceof HTMLInputElement) {
    const blockedTypes = new Set([
      'checkbox',
      'radio',
      'button',
      'submit',
      'reset',
      'file',
      'range',
      'color',
    ]);
    if (blockedTypes.has(target.type) || target.readOnly || target.disabled) {
      return null;
    }
    return target;
  }
  return null;
}

function setNativeInputValue(el: EditableElement, value: string) {
  const proto =
    el instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
  const descriptor = Object.getOwnPropertyDescriptor(proto, 'value');
  if (descriptor?.set) {
    descriptor.set.call(el, value);
  } else {
    el.value = value;
  }
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

export function ContextMenu({
  bootstrap,
  onOpenPalette,
  onStartAll,
  onRestartAll,
  onStopAll,
  onOpenTerminal,
  onOpenWww,
  onRescan,
  onToggleTheme,
  onToggleLang,
  onOpenAbout,
  onNotify,
}: ContextMenuProps) {
  const { t } = useI18n();
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [coords, setCoords] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
      const editableEl = resolveEditable(e.target);

      let selectedText = '';
      if (editableEl) {
        const start = editableEl.selectionStart ?? 0;
        const end = editableEl.selectionEnd ?? 0;
        if (end > start) {
          selectedText = editableEl.value.slice(start, end);
        }
      } else {
        selectedText = window.getSelection()?.toString() ?? '';
      }

      setCoords({ left: e.clientX, top: e.clientY });
      setMenu({
        x: e.clientX,
        y: e.clientY,
        selectedText,
        editableEl,
      });
    };

    const close = () => setMenu(null);
    const handleMouseDown = (e: MouseEvent) => {
      if (menuRef.current && e.target instanceof Node && menuRef.current.contains(e.target)) {
        return;
      }
      setMenu(null);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMenu(null);
    };

    window.addEventListener('contextmenu', handleContextMenu);
    window.addEventListener('mousedown', handleMouseDown);
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    window.addEventListener('scroll', close, true);

    return () => {
      window.removeEventListener('contextmenu', handleContextMenu);
      window.removeEventListener('mousedown', handleMouseDown);
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
      window.removeEventListener('scroll', close, true);
    };
  }, []);

  useLayoutEffect(() => {
    if (!menu || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const pad = 8;
    const left = Math.max(
      pad,
      Math.min(menu.x, window.innerWidth - rect.width - pad),
    );
    const top = Math.max(
      pad,
      Math.min(menu.y, window.innerHeight - rect.height - pad),
    );
    setCoords({ left, top });
  }, [menu]);

  if (!menu) return null;

  const anyInstalled = bootstrap.services.some((s) => s.installed);
  const runningCount = bootstrap.services.filter((s) => s.state === 'running').length;
  const hasSelection = menu.selectedText.length > 0;
  const isEditable = menu.editableEl !== null;

  const runAction = (fn: () => void) => {
    setMenu(null);
    fn();
  };

  const handleCopy = async () => {
    if (!menu.selectedText) return;
    setMenu(null);
    try {
      await navigator.clipboard.writeText(menu.selectedText);
      onNotify(t.contextMenu.copiedToast, 'info');
    } catch {
      // ignore clipboard errors
    }
  };

  const handleCut = async () => {
    const el = menu.editableEl;
    if (!el || !menu.selectedText) return;
    setMenu(null);
    const start = el.selectionStart ?? 0;
    const end = el.selectionEnd ?? 0;
    try {
      await navigator.clipboard.writeText(menu.selectedText);
      const next = el.value.slice(0, start) + el.value.slice(end);
      setNativeInputValue(el, next);
      el.focus();
      el.setSelectionRange(start, start);
    } catch {
      // ignore clipboard errors
    }
  };

  const handlePaste = async () => {
    const el = menu.editableEl;
    if (!el) return;
    setMenu(null);
    try {
      const text = await navigator.clipboard.readText();
      const start = el.selectionStart ?? el.value.length;
      const end = el.selectionEnd ?? el.value.length;
      const next = el.value.slice(0, start) + text + el.value.slice(end);
      setNativeInputValue(el, next);
      el.focus();
      const caret = start + text.length;
      el.setSelectionRange(caret, caret);
    } catch {
      // ignore clipboard errors
    }
  };

  const handleSelectAll = () => {
    const el = menu.editableEl;
    if (!el) return;
    setMenu(null);
    el.focus();
    el.select();
  };

  return (
    <div
      ref={menuRef}
      role="menu"
      className="fixed z-[120] min-w-[230px] rounded-md border p-1 flex flex-col gap-0.5 select-none"
      style={{
        left: `${coords.left}px`,
        top: `${coords.top}px`,
        background: 'var(--bg-elevated)',
        borderColor: 'var(--border)',
        boxShadow: '0 10px 28px rgba(0, 0, 0, 0.45)',
      }}
    >
      {/* Contextual Clipboard Group */}
      {(hasSelection || isEditable) && (
        <>
          {hasSelection && (
            <MenuItemRow
              icon={<Copy size={13} />}
              label={t.contextMenu.copy}
              shortcut="Ctrl+C"
              onClick={() => void handleCopy()}
            />
          )}
          {isEditable && hasSelection && (
            <MenuItemRow
              icon={<Scissors size={13} />}
              label={t.contextMenu.cut}
              shortcut="Ctrl+X"
              onClick={() => void handleCut()}
            />
          )}
          {isEditable && (
            <>
              <MenuItemRow
                icon={<Clipboard size={13} />}
                label={t.contextMenu.paste}
                shortcut="Ctrl+V"
                onClick={() => void handlePaste()}
              />
              <MenuItemRow
                icon={<span className="w-[13px] text-center font-mono text-[10px]">A</span>}
                label={t.contextMenu.selectAll}
                shortcut="Ctrl+A"
                onClick={handleSelectAll}
              />
            </>
          )}
          <div className="my-1 border-t" style={{ borderColor: 'var(--border-subtle)' }} />
        </>
      )}

      {/* App Actions */}
      <MenuItemRow
        icon={<Search size={13} style={{ color: 'var(--accent)' }} />}
        label={t.contextMenu.commandPalette}
        shortcut="Ctrl+K"
        onClick={() => runAction(onOpenPalette)}
      />

      <div className="my-1 border-t" style={{ borderColor: 'var(--border-subtle)' }} />

      <MenuItemRow
        icon={<Play size={13} style={{ color: 'var(--green)' }} />}
        label={t.contextMenu.startAll}
        disabled={!anyInstalled}
        onClick={() => runAction(onStartAll)}
      />
      <MenuItemRow
        icon={<RotateCw size={13} style={{ color: 'var(--amber)' }} />}
        label={t.contextMenu.restartAll}
        disabled={runningCount === 0}
        onClick={() => runAction(onRestartAll)}
      />
      <MenuItemRow
        icon={<Square size={13} style={{ color: 'var(--red)' }} />}
        label={t.contextMenu.stopAll}
        disabled={runningCount === 0}
        onClick={() => runAction(onStopAll)}
      />

      <div className="my-1 border-t" style={{ borderColor: 'var(--border-subtle)' }} />

      <MenuItemRow
        icon={<Terminal size={13} />}
        label={t.contextMenu.openTerminal}
        onClick={() => runAction(onOpenTerminal)}
      />
      <MenuItemRow
        icon={<FolderOpen size={13} />}
        label={t.contextMenu.openWww}
        onClick={() => runAction(onOpenWww)}
      />
      <MenuItemRow
        icon={<RefreshCw size={13} />}
        label={t.contextMenu.rescan}
        onClick={() => runAction(onRescan)}
      />

      <div className="my-1 border-t" style={{ borderColor: 'var(--border-subtle)' }} />

      <MenuItemRow
        icon={
          bootstrap.config.theme === 'dark' ? (
            <Sun size={13} />
          ) : (
            <Moon size={13} />
          )
        }
        label={t.contextMenu.toggleTheme}
        onClick={() => runAction(onToggleTheme)}
      />
      <MenuItemRow
        icon={<Languages size={13} />}
        label={t.contextMenu.switchLang}
        shortcut={bootstrap.config.language === 'en' ? 'ID' : 'EN'}
        onClick={() => runAction(onToggleLang)}
      />
      <MenuItemRow
        icon={<Info size={13} />}
        label={t.contextMenu.about}
        onClick={() => runAction(onOpenAbout)}
      />
    </div>
  );
}

function MenuItemRow({
  icon,
  label,
  shortcut,
  disabled,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
      className="w-full flex items-center justify-between gap-4 px-2.5 py-1.5 rounded text-[12px] text-left cursor-pointer hover:bg-[var(--bg-hover)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
      style={{ color: 'var(--text-primary)' }}
    >
      <span className="flex items-center gap-2 min-w-0">
        <span style={{ color: 'var(--text-muted)' }}>{icon}</span>
        <span className="truncate">{label}</span>
      </span>
      {shortcut && (
        <span className="font-mono text-[10px] shrink-0" style={{ color: 'var(--text-dim)' }}>
          {shortcut}
        </span>
      )}
    </button>
  );
}
