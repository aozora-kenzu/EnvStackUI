import { useEffect, useMemo, useState } from 'react';
import { Bug, FolderOpen, Loader2, RotateCw, Search, SlidersHorizontal, X } from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { PhpExtensionsPayload } from '../types/envstack';

interface PhpExtensionsModalProps {
  open: boolean;
  onClose: () => void;
  onRestartServices: () => void;
  onNotify: (msg: string, kind?: 'ok' | 'err' | 'info') => void;
}

export function PhpExtensionsModal({
  open,
  onClose,
  onRestartServices,
  onNotify,
}: PhpExtensionsModalProps) {
  const { t, format } = useI18n();
  const [payload, setPayload] = useState<PhpExtensionsPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [toggling, setToggling] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setSearch('');
    setError(null);
    setLoading(true);
    void api
      .getPhpExtensions()
      .then(setPayload)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [open]);

  const filtered = useMemo(() => {
    if (!payload) return [];
    const q = search.trim().toLowerCase();
    return payload.extensions.filter(
      (ext) =>
        !q || ext.name.toLowerCase().includes(q) || ext.dll_name.toLowerCase().includes(q),
    );
  }, [payload, search]);

  if (!open) return null;

  const handleToggle = async (name: string, nextEnabled: boolean) => {
    setToggling(name);
    try {
      const updated = await api.togglePhpExtension(name, nextEnabled);
      setPayload(updated);
      onNotify(
        format(t.toasts.extensionUpdated, {
          name,
          state: nextEnabled ? 'ON' : 'OFF',
        }),
        'ok',
      );
    } catch (e) {
      onNotify(String(e), 'err');
    } finally {
      setToggling(null);
    }
  };

  const enabledTotal = payload?.extensions.filter((e) => e.enabled).length ?? 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[660px] rounded-md border overflow-hidden flex flex-col max-h-[85vh]"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-4 py-3 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <div className="flex items-center gap-2">
            <SlidersHorizontal size={15} style={{ color: 'var(--accent)' }} />
            <div>
              <div className="font-semibold text-[14px]">
                {t.phpExt.title}
                {payload ? ` (PHP ${payload.php_version})` : ''}
              </div>
              <div className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                {t.phpExt.subtitle}
              </div>
            </div>
          </div>
          <button type="button" onClick={onClose} className="ui-btn p-1">
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="p-4 flex flex-col gap-3 overflow-y-auto flex-1">
          {loading ? (
            <div
              className="py-10 flex items-center justify-center gap-2 text-[12px]"
              style={{ color: 'var(--text-muted)' }}
            >
              <Loader2 size={15} className="animate-spin" style={{ color: 'var(--accent)' }} />
              <span>Loading php.ini...</span>
            </div>
          ) : error ? (
            <div
              className="p-3 rounded border text-[12px] font-mono selectable"
              style={{
                background: 'var(--red-soft)',
                borderColor: 'var(--red)',
                color: 'var(--red)',
              }}
            >
              {error}
            </div>
          ) : (
            payload && (
              <>
                {/* Xdebug Highlight Card */}
                <div
                  className="p-3 rounded border flex items-center justify-between gap-3 flex-wrap"
                  style={{
                    background: payload.xdebug_enabled ? 'var(--green-soft)' : 'var(--bg-elevated)',
                    borderColor: payload.xdebug_enabled ? 'var(--green)' : 'var(--border)',
                  }}
                >
                  <div className="flex items-start gap-2.5 min-w-0 flex-1">
                    <Bug
                      size={16}
                      className="mt-0.5 shrink-0"
                      style={{
                        color: payload.xdebug_enabled ? 'var(--green)' : 'var(--amber)',
                      }}
                    />
                    <div className="min-w-0">
                      <div className="text-[12px] font-semibold flex items-center gap-2">
                        <span>{t.phpExt.xdebugTitle}</span>
                        {payload.xdebug_available && (
                          <span
                            className="font-mono text-[10px] px-1.5 py-0.2 rounded"
                            style={{
                              background: payload.xdebug_enabled
                                ? 'var(--bg-surface)'
                                : 'var(--bg-app)',
                              color: payload.xdebug_enabled ? 'var(--green)' : 'var(--text-dim)',
                            }}
                          >
                            :{payload.xdebug_port}
                          </span>
                        )}
                      </div>
                      <div className="text-[11px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
                        {!payload.xdebug_available
                          ? t.phpExt.xdebugMissingDesc
                          : payload.xdebug_enabled
                            ? format(t.phpExt.xdebugActiveDesc, { port: payload.xdebug_port })
                            : t.phpExt.xdebugInactiveDesc}
                      </div>
                    </div>
                  </div>

                  {payload.xdebug_available ? (
                    <button
                      type="button"
                      disabled={toggling !== null}
                      onClick={() => void handleToggle('xdebug', !payload.xdebug_enabled)}
                      className={
                        payload.xdebug_enabled
                          ? 'ui-btn ui-btn-danger text-[11px] shrink-0'
                          : 'ui-btn ui-btn-primary text-[11px] shrink-0'
                      }
                    >
                      {toggling === 'xdebug' && <Loader2 size={12} className="animate-spin" />}
                      <span>
                        {payload.xdebug_enabled ? t.phpExt.xdebugDisable : t.phpExt.xdebugEnable}
                      </span>
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void api.openPathInExplorer(payload.ext_dir)}
                      className="ui-btn text-[11px] shrink-0"
                    >
                      <FolderOpen size={12} />
                      <span>{t.phpExt.openExtDir}</span>
                    </button>
                  )}
                </div>

                {/* Search + Counter */}
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <div className="relative flex-1 min-w-[200px]">
                    <Search
                      size={12}
                      className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
                      style={{ color: 'var(--text-dim)' }}
                    />
                    <input
                      type="text"
                      value={search}
                      onChange={(e) => setSearch(e.target.value)}
                      placeholder={t.phpExt.searchPlaceholder}
                      className="ui-input w-full pl-8 font-mono text-[11px]"
                    />
                  </div>
                  <span className="font-mono text-[11px]" style={{ color: 'var(--text-muted)' }}>
                    {format(t.phpExt.enabledCount, {
                      enabled: enabledTotal,
                      total: payload.extensions.length,
                    })}
                  </span>
                </div>

                {/* Extensions Grid */}
                {filtered.length === 0 ? (
                  <div
                    className="py-6 text-center text-[12px]"
                    style={{ color: 'var(--text-dim)' }}
                  >
                    {t.phpExt.noMatches}
                  </div>
                ) : (
                  <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
                    {filtered.map((ext) => {
                      const busy = toggling === ext.name;
                      return (
                        <button
                          key={ext.name}
                          type="button"
                          disabled={toggling !== null}
                          onClick={() => void handleToggle(ext.name, !ext.enabled)}
                          title={ext.dll_name}
                          className="px-2.5 py-1.5 rounded border flex items-center justify-between gap-2 text-left cursor-pointer hover:border-[var(--accent)] disabled:opacity-50 transition-colors"
                          style={{
                            background: ext.enabled ? 'var(--bg-elevated)' : 'var(--bg-input)',
                            borderColor: ext.enabled ? 'var(--green)' : 'var(--border)',
                          }}
                        >
                          <span className="flex items-center gap-1.5 min-w-0">
                            <span
                              className="w-2 h-2 rounded-full shrink-0"
                              style={{
                                background: ext.enabled ? 'var(--green)' : 'var(--text-dim)',
                              }}
                            />
                            <span
                              className="font-mono text-[11px] truncate"
                              style={{
                                color: ext.enabled ? 'var(--text-primary)' : 'var(--text-muted)',
                              }}
                            >
                              {ext.name}
                            </span>
                          </span>
                          {busy ? (
                            <Loader2 size={11} className="animate-spin shrink-0" />
                          ) : (
                            ext.is_zend && (
                              <span
                                className="font-mono text-[9px] px-1 rounded shrink-0"
                                style={{
                                  background: 'var(--accent-soft)',
                                  color: 'var(--accent)',
                                }}
                              >
                                zend
                              </span>
                            )
                          )}
                        </button>
                      );
                    })}
                  </div>
                )}
              </>
            )
          )}
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between gap-2 px-4 py-2.5 border-t flex-wrap"
          style={{ borderColor: 'var(--border)', background: 'var(--bg-sidebar)' }}
        >
          <div className="flex items-center gap-1.5">
            {payload && (
              <>
                <button
                  type="button"
                  onClick={() => void api.openPathInExplorer(payload.ext_dir)}
                  className="ui-btn text-[11px]"
                >
                  <FolderOpen size={12} />
                  <span>{t.phpExt.openExtDir}</span>
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void api.openPathInExplorer(
                      payload.ini_path.replace(/[\\/][^\\/]+$/, ''),
                    )
                  }
                  className="ui-btn text-[11px]"
                >
                  <FolderOpen size={12} />
                  <span>{t.phpExt.openIniFolder}</span>
                </button>
              </>
            )}
          </div>
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => {
                onRestartServices();
                onClose();
              }}
              className="ui-btn text-[11px]"
            >
              <RotateCw size={12} style={{ color: 'var(--amber)' }} />
              <span>{t.phpExt.restartServices}</span>
            </button>
            <button type="button" onClick={onClose} className="ui-btn ui-btn-primary text-[11px]">
              {t.phpExt.close}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
