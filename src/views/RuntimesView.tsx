import { Check, FolderPlus, Loader2, Terminal, Trash2, X } from 'lucide-react';
import { useI18n } from '../i18n';
import type { AppBootstrap } from '../types/envstack';

interface RuntimesViewProps {
  bootstrap: AppBootstrap;
  syncingPath: boolean;
  onSelectVersion: (kind: string, id: string) => void;
  onAddExternalRoot: () => void;
  onRemoveExternalRoot: (path: string) => void;
  onOpenTerminal: (profile?: string) => void;
  onSyncUserPath: () => void;
  onClearUserPath: () => void;
}

const RUNTIME_KINDS = [
  { id: 'php', label: 'PHP CLI' },
  { id: 'node', label: 'Node.js / npm' },
  { id: 'composer', label: 'Composer' },
  { id: 'git', label: 'Git' },
  { id: 'python', label: 'Python' },
  { id: 'go', label: 'Go' },
];

export function RuntimesView({
  bootstrap,
  syncingPath,
  onSelectVersion,
  onAddExternalRoot,
  onRemoveExternalRoot,
  onOpenTerminal,
  onSyncUserPath,
  onClearUserPath,
}: RuntimesViewProps) {
  const { t, format } = useI18n();

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-[16px] font-semibold tracking-tight">{t.runtimes.title}</h1>
        <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
          {t.runtimes.subtitle}
        </p>
      </div>

      {/* Runtime Switcher Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        {RUNTIME_KINDS.map((rt) => {
          const items = bootstrap.inventory.binaries[rt.id] ?? [];
          const activeId = bootstrap.config.active_versions[rt.id] ?? '';
          const activeItem = items.find((i) => i.id === activeId);

          return (
            <div key={rt.id} className="ui-panel p-3 flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-[13px]">{rt.label}</span>
                {activeItem && (
                  <span
                    className="font-mono text-[10px] px-1.5 py-0.5 rounded"
                    style={{
                      background: 'var(--accent-soft)',
                      color: 'var(--accent)',
                    }}
                  >
                    {activeItem.version} ({activeItem.source})
                  </span>
                )}
              </div>

              {items.length > 0 ? (
                <select
                  value={activeId}
                  onChange={(e) => onSelectVersion(rt.id, e.target.value)}
                  className="ui-select font-mono text-[11px] w-full"
                >
                  {items.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.version}
                      {item.variant ? ` (${item.variant})` : ''} — [{item.source}]
                    </option>
                  ))}
                </select>
              ) : (
                <div className="text-[11px] font-mono" style={{ color: 'var(--text-dim)' }}>
                  {t.runtimes.noVersionsFound}
                </div>
              )}

              {activeItem && (
                <div
                  className="font-mono text-[10px] truncate selectable"
                  style={{ color: 'var(--text-dim)' }}
                  title={activeItem.bin_dir}
                >
                  {activeItem.bin_dir}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Portable Terminal + User PATH row */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        <div className="ui-panel p-3.5 flex flex-col justify-between gap-3">
          <div>
            <div className="font-semibold text-[13px]">{t.runtimes.terminalTitle}</div>
            <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {t.runtimes.terminalDesc}
            </p>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <button
              type="button"
              onClick={() => onOpenTerminal()}
              className="ui-btn ui-btn-primary text-[11px]"
            >
              <Terminal size={12} />
              <span>{t.runtimes.openTerminal}</span>
            </button>
            <button
              type="button"
              onClick={() => onOpenTerminal('powershell')}
              className="ui-btn text-[11px]"
            >
              PowerShell
            </button>
            <button
              type="button"
              onClick={() => onOpenTerminal('gitbash')}
              className="ui-btn text-[11px]"
            >
              Git Bash
            </button>
            <button
              type="button"
              onClick={() => onOpenTerminal('cmd')}
              className="ui-btn text-[11px]"
            >
              CMD
            </button>
          </div>
        </div>

        <div className="ui-panel p-3.5 flex flex-col justify-between gap-3">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-semibold text-[13px]">{t.runtimes.userPathTitle}</span>
              <span className="font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>
                {format(t.runtimes.userPathActiveCount, {
                  count: bootstrap.config.user_path_entries.length,
                })}
              </span>
            </div>
            <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {t.runtimes.userPathDesc}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={syncingPath}
              onClick={onSyncUserPath}
              className="ui-btn ui-btn-primary text-[11px]"
            >
              {syncingPath ? <Loader2 size={12} className="animate-spin" /> : <Check size={12} />}
              <span>{t.runtimes.syncUserPath}</span>
            </button>
            <button
              type="button"
              disabled={syncingPath || bootstrap.config.user_path_entries.length === 0}
              onClick={onClearUserPath}
              className="ui-btn ui-btn-danger text-[11px]"
            >
              <X size={12} />
              <span>{t.runtimes.clearUserPath}</span>
            </button>
          </div>
        </div>
      </div>

      {/* External Binary Roots */}
      <div className="ui-panel p-3.5 flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-3">
          <div>
            <div className="font-semibold text-[13px]">{t.runtimes.externalRootsTitle}</div>
            <p className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
              {t.runtimes.externalRootsDesc}
            </p>
          </div>
          <button type="button" onClick={onAddExternalRoot} className="ui-btn text-[11px]">
            <FolderPlus size={12} />
            <span>{t.runtimes.addExternalRoot}</span>
          </button>
        </div>

        {bootstrap.config.external_bin_roots.length === 0 ? (
          <div className="text-[11px] font-mono" style={{ color: 'var(--text-dim)' }}>
            {t.runtimes.noExternalRoots}
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {bootstrap.config.external_bin_roots.map((root) => (
              <div
                key={root}
                className="flex items-center justify-between px-2.5 py-1.5 rounded border"
                style={{ background: 'var(--bg-elevated)', borderColor: 'var(--border)' }}
              >
                <span className="font-mono text-[11px] selectable">{root}</span>
                <button
                  type="button"
                  onClick={() => onRemoveExternalRoot(root)}
                  className="ui-btn ui-btn-danger text-[10px] py-0.5 px-2"
                >
                  <Trash2 size={11} />
                  <span>{t.runtimes.remove}</span>
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
