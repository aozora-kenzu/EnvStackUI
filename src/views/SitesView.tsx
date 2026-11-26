import { useState } from 'react';
import {
  Code2,
  ExternalLink,
  FolderOpen,
  FolderPlus,
  Globe,
  Loader2,
  Radio,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Terminal,
  Unlink,
  X,
} from 'lucide-react';
import { useI18n } from '../i18n';
import { api } from '../lib/api';
import type { AppBootstrap, VhostConfigs } from '../types/envstack';

interface SitesViewProps {
  bootstrap: AppBootstrap;
  syncingHosts: boolean;
  trustingCa: boolean;
  regenerating: boolean;
  busyDns: boolean;
  onSyncHosts: () => void;
  onTrustCa: () => void;
  onRegenerateVhosts: () => void;
  onLinkSite: () => void;
  onUnlinkSite: (path: string) => void;
  onEnableWildcardDns: () => void;
  onDisableWildcardDns: () => void;
  onOpenWww: () => void;
  onOpenFolder: (path: string) => void;
  onOpenTerminal: (cwd: string) => void;
  onOpenUrl: (url: string) => void;
}

export function SitesView({
  bootstrap,
  syncingHosts,
  trustingCa,
  regenerating,
  busyDns,
  onSyncHosts,
  onTrustCa,
  onRegenerateVhosts,
  onLinkSite,
  onUnlinkSite,
  onEnableWildcardDns,
  onDisableWildcardDns,
  onOpenWww,
  onOpenFolder,
  onOpenTerminal,
  onOpenUrl,
}: SitesViewProps) {
  const { t, format } = useI18n();
  const [vhostModal, setVhostModal] = useState<VhostConfigs | null>(null);
  const [vhostTab, setVhostTab] = useState<'nginx' | 'apache'>('nginx');

  const cleanTld = bootstrap.config.tld.trim().replace(/^\./, '') || 'test';
  const isLocalhostTld = cleanTld.toLowerCase() === 'localhost';
  const dns = bootstrap.dns_status;
  const wildcardHealthy = dns.matches_tld && dns.server_running;
  const wildcardMismatch = dns.nrpt_installed && !dns.matches_tld;
  const pendingCount =
    bootstrap.hosts_status.pending_add.length +
    bootstrap.hosts_status.pending_remove.length;

  const inspectVhost = async (domain: string) => {
    const cfg = await api.readSiteVhosts(domain);
    setVhostModal(cfg);
  };

  return (
    <div className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[16px] font-semibold tracking-tight">{t.sites.title}</h1>
          <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {t.sites.subtitle}
          </p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <button type="button" onClick={onLinkSite} className="ui-btn text-[11px]">
            <FolderPlus size={12} />
            <span>{t.sites.linkProject}</span>
          </button>
          <button type="button" onClick={onOpenWww} className="ui-btn text-[11px]">
            <FolderOpen size={12} />
            <span>{t.sites.openWww}</span>
          </button>
          <button
            type="button"
            disabled={regenerating}
            onClick={onRegenerateVhosts}
            className="ui-btn text-[11px]"
          >
            <RefreshCw size={12} className={regenerating ? 'animate-spin' : ''} />
            <span>{t.sites.regenerate}</span>
          </button>
        </div>
      </div>

      {/* Hosts / Wildcard DNS & SSL Status Strip */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        <div className="ui-panel px-3.5 py-2.5 flex items-center justify-between gap-2.5 flex-wrap">
          <div className="flex items-center gap-2.5 min-w-0 flex-1">
            {wildcardHealthy ? (
              <Radio size={15} className="shrink-0" style={{ color: 'var(--green)' }} />
            ) : (
              <Globe
                size={15}
                className="shrink-0"
                style={{
                  color:
                    wildcardMismatch || !bootstrap.hosts_status.in_sync
                      ? 'var(--amber)'
                      : 'var(--green)',
                }}
              />
            )}
            <span className="text-[12px] truncate" style={{ color: 'var(--text-primary)' }}>
              {isLocalhostTld
                ? t.sites.hostsLocalhostNote
                : wildcardHealthy
                  ? format(t.sites.wildcardDnsActive, { tld: cleanTld })
                  : wildcardMismatch
                    ? format(t.sites.wildcardDnsTldMismatch, {
                        current: dns.nrpt_tld ?? '?',
                        active: cleanTld,
                      })
                    : bootstrap.hosts_status.in_sync
                      ? t.sites.hostsInSync
                      : format(t.sites.hostsNeedsSync, { count: pendingCount })}
            </span>
          </div>
          {!isLocalhostTld && (
            <div className="flex items-center gap-1.5 shrink-0">
              {wildcardHealthy ? (
                <button
                  type="button"
                  disabled={busyDns}
                  onClick={onDisableWildcardDns}
                  className="ui-btn text-[11px]"
                >
                  {busyDns && <Loader2 size={11} className="animate-spin" />}
                  <span>{t.sites.disableWildcardDns}</span>
                </button>
              ) : wildcardMismatch ? (
                <>
                  <button
                    type="button"
                    disabled={busyDns}
                    onClick={onEnableWildcardDns}
                    className="ui-btn ui-btn-primary text-[11px]"
                  >
                    {busyDns && <Loader2 size={11} className="animate-spin" />}
                    <span>{t.sites.updateWildcardDns}</span>
                  </button>
                  <button
                    type="button"
                    disabled={busyDns}
                    onClick={onDisableWildcardDns}
                    className="ui-btn text-[11px]"
                  >
                    <span>{t.sites.disableWildcardDns}</span>
                  </button>
                </>
              ) : (
                <>
                  {!bootstrap.hosts_status.in_sync && (
                    <button
                      type="button"
                      disabled={syncingHosts}
                      onClick={onSyncHosts}
                      className="ui-btn ui-btn-primary text-[11px]"
                    >
                      {syncingHosts && <Loader2 size={11} className="animate-spin" />}
                      <span>{t.sites.syncHosts}</span>
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={busyDns}
                    onClick={onEnableWildcardDns}
                    title={format(t.sites.wildcardDnsActive, { tld: cleanTld })}
                    className="ui-btn text-[11px]"
                  >
                    {busyDns && <Loader2 size={11} className="animate-spin" />}
                    <span>{t.sites.enableWildcardDns}</span>
                  </button>
                </>
              )}
            </div>
          )}
        </div>

        <div className="ui-panel px-3.5 py-2.5 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2.5 min-w-0">
            {bootstrap.ssl_status.ca_trusted ? (
              <ShieldCheck size={15} style={{ color: 'var(--green)' }} />
            ) : (
              <ShieldAlert size={15} style={{ color: 'var(--amber)' }} />
            )}
            <span className="text-[12px] truncate" style={{ color: 'var(--text-primary)' }}>
              {bootstrap.ssl_status.ca_trusted
                ? t.sites.caTrustedMsg
                : t.sites.caUntrustedMsg}
            </span>
          </div>
          {!bootstrap.ssl_status.ca_trusted && (
            <button
              type="button"
              disabled={trustingCa}
              onClick={onTrustCa}
              className="ui-btn ui-btn-primary text-[11px] shrink-0"
            >
              {trustingCa && <Loader2 size={11} className="animate-spin" />}
              <span>{t.sites.trustCa}</span>
            </button>
          )}
        </div>
      </div>

      {/* Sites Table */}
      {bootstrap.sites.length === 0 ? (
        <div className="ui-panel p-8 text-center">
          <div className="text-[13px] font-semibold">{t.sites.emptyTitle}</div>
          <p className="text-[12px] mt-1" style={{ color: 'var(--text-muted)' }}>
            {t.sites.emptyDesc}
          </p>
          <div className="font-mono text-[11px] mt-2" style={{ color: 'var(--text-dim)' }}>
            {bootstrap.www_root}
          </div>
        </div>
      ) : (
        <div className="ui-panel overflow-hidden">
          <table className="w-full text-left border-collapse text-[12px]">
            <thead>
              <tr
                className="border-b text-[11px]"
                style={{
                  background: 'var(--bg-elevated)',
                  borderColor: 'var(--border)',
                  color: 'var(--text-muted)',
                }}
              >
                <th className="py-2 px-3 font-medium">{t.sites.colProject}</th>
                <th className="py-2 px-3 font-medium">{t.sites.colDomain}</th>
                <th className="py-2 px-3 font-medium">{t.sites.colFramework}</th>
                <th className="py-2 px-3 font-medium">{t.sites.colSsl}</th>
                <th className="py-2 px-3 font-medium text-right">{t.sites.colActions}</th>
              </tr>
            </thead>
            <tbody>
              {bootstrap.sites.map((site) => {
                const preferredUrl =
                  site.has_ssl && bootstrap.config.auto_ssl ? site.https_url : site.http_url;
                const preferredPort = (() => {
                  try {
                    return new URL(preferredUrl).port;
                  } catch {
                    return '';
                  }
                })();
                return (
                <tr
                  key={site.domain}
                  className="border-b last:border-b-0 hover:bg-[var(--bg-hover)]"
                  style={{ borderColor: 'var(--border-subtle)' }}
                >
                  <td className="py-2.5 px-3 font-semibold">
                    <div className="flex items-center gap-1.5">
                      <span>{site.name}</span>
                      {site.is_linked && (
                        <span
                          className="font-mono text-[10px] px-1.5 py-0.2 rounded font-normal"
                          style={{
                            background: 'var(--accent-soft)',
                            color: 'var(--accent)',
                          }}
                          title={site.path}
                        >
                          {t.sites.linkedBadge}
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="py-2.5 px-3">
                    <button
                      type="button"
                      title={preferredUrl}
                      onClick={() => onOpenUrl(preferredUrl)}
                      className="font-mono text-[12px] flex items-center gap-1 cursor-pointer hover:underline"
                      style={{ color: 'var(--accent)' }}
                    >
                      <span>
                        {site.domain}
                        {preferredPort ? `:${preferredPort}` : ''}
                      </span>
                      <ExternalLink size={11} />
                    </button>
                  </td>
                  <td className="py-2.5 px-3">
                    <div className="flex items-center gap-2">
                      <span
                        className="font-mono text-[10px] px-1.5 py-0.5 rounded"
                        style={{
                          background: 'var(--bg-elevated)',
                          color: 'var(--text-muted)',
                        }}
                      >
                        {site.framework}
                      </span>
                      {site.doc_root !== site.path && (
                        <span className="font-mono text-[10px]" style={{ color: 'var(--green)' }}>
                          /public
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="py-2.5 px-3">
                    <span
                      className="font-mono text-[10px] px-1.5 py-0.5 rounded"
                      style={{
                        background: site.has_ssl ? 'var(--green-soft)' : 'var(--bg-elevated)',
                        color: site.has_ssl ? 'var(--green)' : 'var(--text-dim)',
                      }}
                    >
                      {site.has_ssl ? t.sites.sslReady : t.sites.sslMissing}
                    </span>
                  </td>
                  <td className="py-2.5 px-3">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        type="button"
                        onClick={() => onOpenUrl(site.http_url)}
                        title={`${t.sites.openHttp} (${site.http_url})`}
                        className="ui-btn text-[10px] px-2 py-1 font-mono"
                      >
                        HTTP
                      </button>
                      {site.has_ssl && (
                        <button
                          type="button"
                          onClick={() => onOpenUrl(site.https_url)}
                          title={`${t.sites.openHttps} (${site.https_url})`}
                          className="ui-btn text-[10px] px-2 py-1 font-mono"
                          style={{ color: 'var(--green)' }}
                        >
                          HTTPS
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => onOpenFolder(site.path)}
                        title={t.sites.openFolder}
                        className="ui-btn p-1.5"
                      >
                        <FolderOpen size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => onOpenTerminal(site.path)}
                        title={t.sites.openTerminal}
                        className="ui-btn p-1.5"
                      >
                        <Terminal size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => inspectVhost(site.domain)}
                        title={t.sites.viewVhost}
                        className="ui-btn p-1.5"
                      >
                        <Code2 size={12} />
                      </button>
                      {site.is_linked && (
                        <button
                          type="button"
                          onClick={() => onUnlinkSite(site.path)}
                          title={`${t.sites.unlinkProject} (${site.path})`}
                          className="ui-btn ui-btn-danger p-1.5"
                        >
                          <Unlink size={12} />
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {/* VHost Config Preview Modal */}
      {vhostModal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          onClick={() => setVhostModal(null)}
        >
          <div
            className="w-full max-w-[680px] rounded-md border overflow-hidden"
            style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="flex items-center justify-between px-4 py-2.5 border-b"
              style={{ borderColor: 'var(--border)' }}
            >
              <span className="font-semibold text-[13px]">
                {t.sites.vhostModalTitle} — <span className="font-mono">{vhostModal.domain}</span>
              </span>
              <button
                type="button"
                onClick={() => setVhostModal(null)}
                className="ui-btn p-1"
              >
                <X size={14} />
              </button>
            </div>
            <div className="px-4 pt-2 flex gap-2 border-b" style={{ borderColor: 'var(--border)' }}>
              {(['nginx', 'apache'] as const).map((tab) => (
                <button
                  key={tab}
                  type="button"
                  onClick={() => setVhostTab(tab)}
                  className="px-3 py-1.5 text-[12px] font-mono border-b-2 cursor-pointer"
                  style={{
                    borderColor: vhostTab === tab ? 'var(--accent)' : 'transparent',
                    color: vhostTab === tab ? 'var(--text-primary)' : 'var(--text-muted)',
                  }}
                >
                  {tab === 'nginx' ? 'Nginx' : 'Apache'}
                </button>
              ))}
            </div>
            <pre
              className="p-4 font-mono text-[11px] max-h-[380px] overflow-auto selectable whitespace-pre-wrap"
              style={{ background: 'var(--bg-input)', color: 'var(--text-primary)' }}
            >
              {(vhostTab === 'nginx' ? vhostModal.nginx_conf : vhostModal.apache_conf) ||
                '// Click "Regenerate Configs" to generate this file.'}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}
