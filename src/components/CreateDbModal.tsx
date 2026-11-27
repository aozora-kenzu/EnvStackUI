import { useEffect, useState } from 'react';
import { Database, Loader2, Plus, RefreshCw, X } from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';

interface CreateDbModalProps {
  open: boolean;
  engine: 'mysql' | 'postgres';
  initialName?: string;
  onClose: () => void;
  onCreated: (name: string, engine: 'mysql' | 'postgres') => void;
}

export function CreateDbModal({
  open,
  engine,
  initialName = '',
  onClose,
  onCreated,
}: CreateDbModalProps) {
  const { t } = useI18n();
  const [name, setName] = useState(initialName);
  const [databases, setDatabases] = useState<string[]>([]);
  const [loadingList, setLoadingList] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadDatabases = async () => {
    setLoadingList(true);
    setError(null);
    try {
      const list = await api.listDatabases(engine);
      setDatabases(list);
    } catch (e) {
      setDatabases([]);
      setError(String(e));
    } finally {
      setLoadingList(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    setName(initialName);
    void loadDatabases();
  }, [open, engine, initialName]);

  if (!open) return null;

  const engineLabel = engine === 'postgres' ? 'PostgreSQL' : 'MariaDB / MySQL';

  const handleCreate = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setCreating(true);
    setError(null);
    try {
      const updated = await api.createDatabase(engine, trimmed);
      setDatabases(updated);
      onCreated(trimmed, engine);
      setName('');
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4"
      onClick={() => !creating && onClose()}
    >
      <div
        className="w-full max-w-[460px] rounded-lg border overflow-hidden"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="flex items-center justify-between px-4 py-3 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <div className="flex items-center gap-2">
            <Database size={15} style={{ color: 'var(--accent)' }} />
            <span className="font-semibold text-[14px]">{t.createDb.title}</span>
            <span
              className="font-mono text-[10px] px-1.5 py-0.5 rounded"
              style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
            >
              {engineLabel}
            </span>
          </div>
          <button
            type="button"
            disabled={creating}
            onClick={onClose}
            className="ui-btn p-1"
            aria-label={t.about.close}
          >
            <X size={14} />
          </button>
        </div>

        <div className="p-4 flex flex-col gap-3">
          <div className="flex flex-col gap-1 text-[12px]">
            <span style={{ color: 'var(--text-muted)' }}>{t.createDb.nameLabel}</span>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !creating && name.trim()) {
                    void handleCreate();
                  }
                }}
                placeholder={t.createDb.namePlaceholder}
                className="ui-input font-mono flex-1"
                autoFocus
              />
              <button
                type="button"
                disabled={creating || !name.trim()}
                onClick={() => void handleCreate()}
                className="ui-btn ui-btn-primary text-[12px]"
              >
                {creating ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <Plus size={13} />
                )}
                <span>{t.createDb.createBtn}</span>
              </button>
            </div>
          </div>

          {error && (
            <div
              className="font-mono text-[11px] px-2.5 py-2 rounded border selectable"
              style={{
                background: 'var(--red-soft)',
                borderColor: 'var(--red)',
                color: 'var(--red)',
              }}
            >
              {error}
            </div>
          )}

          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-medium" style={{ color: 'var(--text-muted)' }}>
                {t.createDb.existingTitle} ({databases.length})
              </span>
              <button
                type="button"
                disabled={loadingList}
                onClick={() => void loadDatabases()}
                className="ui-btn text-[10px] py-0.5 px-2"
              >
                <RefreshCw size={11} className={loadingList ? 'animate-spin' : ''} />
                <span>{t.logs.refresh}</span>
              </button>
            </div>

            <div
              className="rounded border p-2 max-h-[180px] overflow-y-auto"
              style={{ background: 'var(--bg-input)', borderColor: 'var(--border)' }}
            >
              {loadingList ? (
                <div
                  className="flex items-center justify-center gap-2 py-4 text-[11px]"
                  style={{ color: 'var(--text-muted)' }}
                >
                  <Loader2 size={13} className="animate-spin" />
                  <span>{t.createDb.loading}</span>
                </div>
              ) : databases.length === 0 ? (
                <div
                  className="text-center py-4 text-[11px]"
                  style={{ color: 'var(--text-dim)' }}
                >
                  {t.createDb.empty}
                </div>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {databases.map((db) => (
                    <span
                      key={db}
                      className="font-mono text-[11px] px-2 py-0.5 rounded border selectable"
                      style={{
                        background: 'var(--bg-surface)',
                        borderColor: 'var(--border)',
                        color: 'var(--text-primary)',
                      }}
                    >
                      {db}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
