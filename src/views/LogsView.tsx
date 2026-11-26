import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { FolderOpen, RefreshCw, Search, Trash2 } from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { LogFileInfo } from '../types/envstack';

interface LogsViewProps {
  onOpenLogsFolder: () => void;
  onNotify: (msg: string, kind?: 'ok' | 'err' | 'info') => void;
}

export function LogsView({ onOpenLogsFolder, onNotify }: LogsViewProps) {
  const { t, format } = useI18n();
  const [files, setFiles] = useState<LogFileInfo[]>([]);
  const [selected, setSelected] = useState<string>('envstack.log');
  const [content, setContent] = useState<string>('');
  const [totalLines, setTotalLines] = useState<number>(0);
  const [filter, setFilter] = useState<string>('');
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [refreshing, setRefreshing] = useState<boolean>(false);
  const [clearing, setClearing] = useState<boolean>(false);
  const preRef = useRef<HTMLPreElement>(null);

  const loadLogsList = async () => {
    const list = await api.listLogs();
    setFiles(list);
  };

  const loadLogContent = async (name: string) => {
    const tail = await api.readLogTail(name, 400);
    setContent(tail.content);
    setTotalLines(tail.total_lines);
    return tail;
  };

  useEffect(() => {
    void loadLogsList();
    void loadLogContent(selected);

    const timer = setInterval(() => {
      void loadLogContent(selected);
    }, 2500);

    const unlistenPromise = listen<string>('log-appended', () => {
      if (selected === 'envstack.log') {
        void loadLogContent('envstack.log');
      }
    });

    return () => {
      clearInterval(timer);
      void unlistenPromise.then((fn) => fn());
    };
  }, [selected]);

  useEffect(() => {
    if (autoScroll && preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [content, autoScroll]);

  const filteredLines = useMemo(() => {
    if (!content) return '';
    const q = filter.trim().toLowerCase();
    if (!q) return content;
    return content
      .split('\n')
      .filter((l) => l.toLowerCase().includes(q))
      .join('\n');
  }, [content, filter]);

  const handleManualRefresh = async () => {
    setRefreshing(true);
    try {
      const [tail] = await Promise.all([
        loadLogContent(selected),
        loadLogsList(),
        new Promise((r) => setTimeout(r, 320)),
      ]);
      onNotify(
        format(t.toasts.logsRefreshed, { name: selected, count: tail.total_lines }),
        'ok',
      );
    } finally {
      setRefreshing(false);
    }
  };

  const handleClear = async () => {
    setClearing(true);
    try {
      await api.clearLog(selected);
      await loadLogContent(selected);
      await loadLogsList();
      onNotify(format(t.toasts.logsCleared, { name: selected }), 'info');
    } finally {
      setClearing(false);
    }
  };

  return (
    <div className="flex flex-col gap-3 h-full">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.logs.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.logs.subtitle}
          </p>
        </div>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            disabled={refreshing}
            onClick={handleManualRefresh}
            className="ui-btn text-[11px]"
          >
            <RefreshCw size={12} className={refreshing ? 'animate-spin' : ''} />
            <span>{t.logs.refresh}</span>
          </button>
          <button
            type="button"
            disabled={clearing}
            onClick={handleClear}
            className="ui-btn ui-btn-danger text-[11px]"
          >
            <Trash2 size={12} />
            <span>{t.logs.clear}</span>
          </button>
          <button type="button" onClick={onOpenLogsFolder} className="ui-btn text-[11px]">
            <FolderOpen size={12} />
            <span>{t.logs.openFolder}</span>
          </button>
        </div>
      </div>

      {/* Log file tabs */}
      <div className="flex items-center gap-1 flex-wrap">
        {files.map((f) => {
          const active = selected === f.name;
          return (
            <button
              key={f.name}
              type="button"
              onClick={() => setSelected(f.name)}
              className="px-2.5 py-1 rounded border text-[11px] font-mono cursor-pointer flex items-center gap-1.5 transition-colors"
              style={{
                background: active ? 'var(--bg-active)' : 'var(--bg-surface)',
                borderColor: active ? 'var(--accent)' : 'var(--border)',
                color: active ? 'var(--text-primary)' : 'var(--text-muted)',
                opacity: f.exists ? 1 : 0.6,
              }}
            >
              <span>{f.name}</span>
              {f.size_bytes > 0 && (
                <span className="text-[10px]" style={{ color: 'var(--text-dim)' }}>
                  {(f.size_bytes / 1024).toFixed(1)}k
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Filter & Auto-scroll */}
      <div className="flex items-center justify-between gap-3">
        <div className="relative flex-1 max-w-[340px]">
          <Search
            size={12}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
            style={{ color: 'var(--text-dim)' }}
          />
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={t.logs.searchPlaceholder}
            className="ui-input w-full pl-8 font-mono text-[11px]"
          />
        </div>
        <div className="flex items-center gap-3 text-[11px]">
          <span className="font-mono" style={{ color: 'var(--text-dim)' }}>
            {format(t.logs.linesCount, { count: totalLines })}
          </span>
          <label className="flex items-center gap-1.5 cursor-pointer">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
            />
            <span>{t.logs.autoScroll}</span>
          </label>
        </div>
      </div>

      {/* Log Output */}
      <pre
        ref={preRef}
        className="ui-panel flex-1 min-h-[280px] p-3 font-mono text-[11px] overflow-auto selectable whitespace-pre-wrap"
        style={{
          background: 'var(--bg-input)',
          color: filteredLines ? 'var(--text-primary)' : 'var(--text-dim)',
        }}
      >
        {filteredLines || t.logs.emptyLog}
      </pre>
    </div>
  );
}
