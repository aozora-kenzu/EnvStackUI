import { useEffect, useState } from 'react';
import { Check, Copy, FolderOpen, Loader2, Save, ShieldAlert } from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { AppBootstrap, AppConfig, LanguageCode, ThemeMode } from '../types/envstack';

interface SettingsViewProps {
  bootstrap: AppBootstrap;
  onSaveConfig: (cfg: AppConfig) => Promise<void>;
  onRegenerateCa: () => Promise<void>;
}

export function SettingsView({
  bootstrap,
  onSaveConfig,
  onRegenerateCa,
}: SettingsViewProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<AppConfig>(bootstrap.config);
  const [saving, setSaving] = useState(false);
  const [savedNotice, setSavedNotice] = useState(false);
  const [copiedMcp, setCopiedMcp] = useState(false);

  useEffect(() => {
    setDraft(bootstrap.config);
  }, [bootstrap.config]);

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSaveConfig(draft);
      setSavedNotice(true);
      setTimeout(() => setSavedNotice(false), 2000);
    } finally {
      setSaving(false);
    }
  };

  const pickWwwRoot = async () => {
    const dir = await api.pickDirectory(t.settings.wwwRootLabel);
    if (dir) setDraft((prev) => ({ ...prev, www_root: dir }));
  };

  const pickDbClient = async () => {
    const exe = await api.pickExecutable(t.settings.dbClientPath);
    if (exe) setDraft((prev) => ({ ...prev, db_client_path: exe }));
  };

  const setPort = (key: keyof AppConfig['ports'], val: string) => {
    const num = Number.parseInt(val, 10);
    if (Number.isNaN(num) || num < 1 || num > 65535) return;
    setDraft((prev) => ({
      ...prev,
      ports: { ...prev.ports, [key]: num },
    }));
  };

  return (
    <div className="flex flex-col gap-4 pb-6">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.settings.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.settings.subtitle}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {savedNotice && (
            <span className="text-[11px] font-mono" style={{ color: 'var(--green)' }}>
              {t.settings.savedToast}
            </span>
          )}
          <button
            type="button"
            disabled={saving}
            onClick={handleSave}
            className="ui-btn ui-btn-primary text-[12px]"
          >
            {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
            <span>{t.settings.saveSettings}</span>
          </button>
        </div>
      </div>

      {/* Appearance & Language */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <h2 className="text-[13px] font-semibold">{t.settings.appearanceSection}</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <label className="flex flex-col gap-1 text-[12px]">
            <span style={{ color: 'var(--text-muted)' }}>{t.settings.themeLabel}</span>
            <select
              value={draft.theme}
              onChange={(e) =>
                setDraft((p) => ({ ...p, theme: e.target.value as ThemeMode }))
              }
              className="ui-select"
            >
              <option value="dark">{t.settings.themeDark}</option>
              <option value="light">{t.settings.themeLight}</option>
            </select>
          </label>
          <label className="flex flex-col gap-1 text-[12px]">
            <span style={{ color: 'var(--text-muted)' }}>{t.settings.languageLabel}</span>
            <select
              value={draft.language}
              onChange={(e) =>
                setDraft((p) => ({ ...p, language: e.target.value as LanguageCode }))
              }
              className="ui-select"
            >
              <option value="en">English (Default)</option>
              <option value="id">Bahasa Indonesia</option>
            </select>
          </label>
        </div>
      </section>

      {/* General & Document Root */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <h2 className="text-[13px] font-semibold">{t.settings.generalSection}</h2>
        <div className="flex flex-col gap-1 text-[12px]">
          <span style={{ color: 'var(--text-muted)' }}>{t.settings.wwwRootLabel}</span>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={draft.www_root ?? ''}
              placeholder={bootstrap.www_root}
              onChange={(e) =>
                setDraft((p) => ({
                  ...p,
                  www_root: e.target.value.trim() ? e.target.value : null,
                }))
              }
              className="ui-input font-mono flex-1"
            />
            <button type="button" onClick={pickWwwRoot} className="ui-btn text-[11px]">
              <FolderOpen size={12} />
              <span>{t.settings.browse}</span>
            </button>
            <button
              type="button"
              onClick={() => setDraft((p) => ({ ...p, www_root: null }))}
              className="ui-btn text-[11px]"
            >
              {t.settings.resetDefault}
            </button>
          </div>
        </div>

        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.minimize_to_tray}
            onChange={(e) =>
              setDraft((p) => ({ ...p, minimize_to_tray: e.target.checked }))
            }
          />
          <span>{t.settings.minimizeToTray}</span>
        </label>

        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.start_on_launch}
            onChange={(e) =>
              setDraft((p) => ({ ...p, start_on_launch: e.target.checked }))
            }
          />
          <span>{t.settings.startOnLaunch}</span>
        </label>

        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.mailpit_notifications}
            onChange={(e) =>
              setDraft((p) => ({ ...p, mailpit_notifications: e.target.checked }))
            }
          />
          <span>{t.settings.mailpitNotifications}</span>
        </label>
      </section>

      {/* Virtual Hosts & SSL */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h2 className="text-[13px] font-semibold">{t.settings.vhostSection}</h2>
          <button
            type="button"
            onClick={onRegenerateCa}
            className="ui-btn text-[11px]"
          >
            <ShieldAlert size={12} style={{ color: 'var(--amber)' }} />
            <span>{t.settings.regenerateCa}</span>
          </button>
        </div>

        <label className="flex flex-col gap-1 text-[12px] max-w-[240px]">
          <span style={{ color: 'var(--text-muted)' }}>{t.settings.tldLabel}</span>
          <input
            type="text"
            value={draft.tld}
            onChange={(e) => setDraft((p) => ({ ...p, tld: e.target.value }))}
            className="ui-input font-mono"
            placeholder="test"
          />
        </label>

        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.auto_vhost}
            onChange={(e) =>
              setDraft((p) => ({ ...p, auto_vhost: e.target.checked }))
            }
          />
          <span>{t.settings.autoVhost}</span>
        </label>

        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.auto_ssl}
            onChange={(e) =>
              setDraft((p) => ({ ...p, auto_ssl: e.target.checked }))
            }
          />
          <span>{t.settings.autoSsl}</span>
        </label>
      </section>

      {/* Ports */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <h2 className="text-[13px] font-semibold">{t.settings.portsSection}</h2>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-[12px]">
          {(
            [
              ['apache_http', 'Apache HTTP'],
              ['apache_https', 'Apache HTTPS'],
              ['nginx_http', 'Nginx HTTP'],
              ['nginx_https', 'Nginx HTTPS'],
              ['php_cgi', 'PHP FastCGI'],
              ['mysql', 'MariaDB / MySQL'],
              ['postgres', 'PostgreSQL'],
              ['redis', 'Redis'],
              ['mailpit_http', 'Mailpit UI'],
              ['mailpit_smtp', 'Mailpit SMTP'],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="flex flex-col gap-1">
              <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                {label}
              </span>
              <input
                type="number"
                value={draft.ports[key]}
                onChange={(e) => setPort(key, e.target.value)}
                className="ui-input font-mono"
              />
            </label>
          ))}
        </div>
      </section>

      {/* Terminal & DB Client */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <h2 className="text-[13px] font-semibold">{t.settings.toolsSection}</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-[12px]">
          <label className="flex flex-col gap-1">
            <span style={{ color: 'var(--text-muted)' }}>{t.settings.terminalProfile}</span>
            <select
              value={draft.terminal_profile}
              onChange={(e) =>
                setDraft((p) => ({ ...p, terminal_profile: e.target.value }))
              }
              className="ui-select"
            >
              <option value="wt">Windows Terminal</option>
              <option value="powershell">PowerShell</option>
              <option value="gitbash">Git Bash</option>
              <option value="cmd">Command Prompt (CMD)</option>
            </select>
          </label>

          <div className="flex flex-col gap-1">
            <span style={{ color: 'var(--text-muted)' }}>{t.settings.dbClientPath}</span>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={draft.db_client_path ?? ''}
                placeholder={
                  bootstrap.inventory.detected_db_clients[0]?.exe_path ??
                  t.settings.autoDetect
                }
                onChange={(e) =>
                  setDraft((p) => ({
                    ...p,
                    db_client_path: e.target.value.trim() ? e.target.value : null,
                  }))
                }
                className="ui-input font-mono flex-1"
              />
              <button type="button" onClick={pickDbClient} className="ui-btn text-[11px]">
                {t.settings.browse}
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* Updates & GitHub Repo */}
      <section className="ui-panel p-4 flex flex-col gap-3">
        <h2 className="text-[13px] font-semibold">{t.settings.updatesSection}</h2>
        <label className="flex flex-col gap-1 text-[12px] max-w-[340px]">
          <span style={{ color: 'var(--text-muted)' }}>{t.settings.githubRepo}</span>
          <input
            type="text"
            value={draft.github_repo}
            onChange={(e) =>
              setDraft((p) => ({ ...p, github_repo: e.target.value }))
            }
            className="ui-input font-mono"
          />
        </label>
        <label className="flex items-center gap-2 text-[12px] cursor-pointer">
          <input
            type="checkbox"
            checked={draft.auto_check_updates}
            onChange={(e) =>
              setDraft((p) => ({ ...p, auto_check_updates: e.target.checked }))
            }
          />
          <span>{t.settings.autoCheckUpdates}</span>
        </label>
      </section>

      {/* CLI & AI MCP Integration */}
      <section className="ui-panel p-4 flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div>
            <h2 className="text-[13px] font-semibold">{t.settings.mcpSection}</h2>
            <p className="text-[11px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {t.settings.mcpDesc}
            </p>
          </div>
          <button
            type="button"
            onClick={() => {
              const snippet = JSON.stringify(
                {
                  mcpServers: {
                    envstack: {
                      command: `${bootstrap.root_dir}\\envstack-ui.exe`,
                      args: ['mcp'],
                    },
                  },
                },
                null,
                2,
              );
              void navigator.clipboard.writeText(snippet).then(() => {
                setCopiedMcp(true);
                setTimeout(() => setCopiedMcp(false), 1800);
              });
            }}
            className="ui-btn text-[11px]"
          >
            {copiedMcp ? <Check size={12} style={{ color: 'var(--green)' }} /> : <Copy size={12} />}
            <span>{copiedMcp ? t.settings.mcpCopied : t.settings.mcpCopyBtn}</span>
          </button>
        </div>
        <pre
          className="p-3 rounded border font-mono text-[11px] overflow-x-auto selectable"
          style={{
            background: 'var(--bg-input)',
            borderColor: 'var(--border)',
            color: 'var(--text-primary)',
          }}
        >
          {JSON.stringify(
            {
              mcpServers: {
                envstack: {
                  command: `${bootstrap.root_dir}\\envstack-ui.exe`,
                  args: ['mcp'],
                },
              },
            },
            null,
            2,
          )}
        </pre>
      </section>
    </div>
  );
}
