import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { AlertCircle, CheckCircle2, Info, X } from 'lucide-react';
import { AboutModal } from './components/AboutModal';
import { CommandPalette } from './components/CommandPalette';
import { ContextMenu } from './components/ContextMenu';
import { PhpExtensionsModal } from './components/PhpExtensionsModal';
import { Sidebar } from './components/Sidebar';
import { StatusBar } from './components/StatusBar';
import { Titlebar } from './components/Titlebar';
import { I18nProvider } from './i18n';
import { en } from './i18n/en';
import { id as idDict } from './i18n/id';
import { api } from './lib/api';
import type {
  AppBootstrap,
  AppConfig,
  DownloadProgress,
  LanguageCode,
  MailpitUnreadPayload,
  NavTab,
  OnlineVersionItem,
  ThemeMode,
  UpdateCheckResult,
} from './types/envstack';
import { LogsView } from './views/LogsView';
import { OverviewView } from './views/OverviewView';
import { PackagesView } from './views/PackagesView';
import { RuntimesView } from './views/RuntimesView';
import { ServicesView } from './views/ServicesView';
import { SettingsView } from './views/SettingsView';
import { SitesView } from './views/SitesView';

const minDelay = (ms = 320) => new Promise((resolve) => setTimeout(resolve, ms));

export function App() {
  const [bootstrap, setBootstrap] = useState<AppBootstrap | null>(null);
  const [activeTab, setActiveTab] = useState<NavTab>('overview');
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [phpExtOpen, setPhpExtOpen] = useState(false);
  const [busyService, setBusyService] = useState<string | null>(null);
  const [busyAll, setBusyAll] = useState(false);
  const [rescanning, setRescanning] = useState(false);
  const [syncingHosts, setSyncingHosts] = useState(false);
  const [busyDns, setBusyDns] = useState(false);
  const [trustingCa, setTrustingCa] = useState(false);
  const [regenerating, setRegenerating] = useState(false);
  const [syncingPath, setSyncingPath] = useState(false);
  const [toast, setToast] = useState<{ msg: string; kind: 'ok' | 'err' | 'info' } | null>(null);
  const [cachedUpdate, setCachedUpdate] = useState<UpdateCheckResult | null>(null);
  const [mailpitUnread, setMailpitUnread] = useState(0);
  const [installingIds, setInstallingIds] = useState<string[]>([]);
  const [progressMap, setProgressMap] = useState<Record<string, DownloadProgress>>({});

  const showToast = useCallback((msg: string, kind: 'ok' | 'err' | 'info' = 'ok') => {
    setToast({ msg, kind });
    setTimeout(() => {
      setToast((curr) => (curr?.msg === msg ? null : curr));
    }, 3400);
  }, []);

  const refreshBootstrap = useCallback(async () => {
    const data = await api.getBootstrap();
    setBootstrap(data);
  }, []);

  useEffect(() => {
    void refreshBootstrap();
  }, [refreshBootstrap]);

  useEffect(() => {
    const theme: ThemeMode = bootstrap?.config.theme === 'light' ? 'light' : 'dark';
    document.documentElement.setAttribute('data-theme', theme);
  }, [bootstrap?.config.theme]);

  useEffect(() => {
    if (!bootstrap?.config.auto_check_updates) return;
    void api.checkForUpdates().then(setCachedUpdate).catch(() => {});
  }, [bootstrap?.config.github_repo]);

  useEffect(() => {
    const u1 = listen('service-status-changed', () => {
      void refreshBootstrap();
    });
    const u2 = listen('sites-changed', () => {
      void refreshBootstrap();
    });
    const u3 = listen('open-about', () => {
      setAboutOpen(true);
    });
    const u4 = listen<MailpitUnreadPayload>('mailpit-unread', (ev) => {
      setMailpitUnread(ev.payload.unread);
    });
    const u5 = listen<DownloadProgress>('download-progress', (ev) => {
      setProgressMap((prev) => ({
        ...prev,
        [ev.payload.package_id]: ev.payload,
      }));
    });

    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener('keydown', onKeyDown);

    return () => {
      window.removeEventListener('keydown', onKeyDown);
      void u1.then((f) => f());
      void u2.then((f) => f());
      void u3.then((f) => f());
      void u4.then((f) => f());
      void u5.then((f) => f());
    };
  }, [refreshBootstrap]);

  if (!bootstrap) {
    return (
      <div
        className="h-full w-full flex items-center justify-center font-mono text-[12px]"
        style={{ background: 'var(--bg-app)', color: 'var(--text-muted)' }}
      >
        Loading EnvStackUI...
      </div>
    );
  }

  const dict = bootstrap.config.language === 'id' ? idDict : en;
  const fmt = (tpl: string, vars: Record<string, string | number>) =>
    Object.entries(vars).reduce((acc, [k, v]) => acc.replaceAll(`{${k}}`, String(v)), tpl);

  const updateConfig = async (cfg: AppConfig) => {
    const updated = await api.updateConfig(cfg);
    setBootstrap(updated);
    showToast(dict.toasts.settingsSaved, 'ok');
  };

  const handleToggleTheme = async () => {
    const next: ThemeMode = bootstrap.config.theme === 'dark' ? 'light' : 'dark';
    const updated = await api.updateConfig({ ...bootstrap.config, theme: next });
    setBootstrap(updated);
  };

  const handleToggleLang = async () => {
    const next: LanguageCode = bootstrap.config.language === 'en' ? 'id' : 'en';
    const updated = await api.updateConfig({ ...bootstrap.config, language: next });
    setBootstrap(updated);
  };

  const handleStartService = async (id: string) => {
    setBusyService(id);
    try {
      const services = await api.startService(id);
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(fmt(dict.toasts.serviceStarted, { name: id }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
      await refreshBootstrap();
    } finally {
      setBusyService(null);
    }
  };

  const handleStopService = async (id: string) => {
    setBusyService(id);
    try {
      const services = await api.stopService(id);
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(fmt(dict.toasts.serviceStopped, { name: id }), 'info');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setBusyService(null);
    }
  };

  const handleRestartService = async (id: string) => {
    setBusyService(id);
    try {
      const services = await api.restartService(id);
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(fmt(dict.toasts.serviceRestarted, { name: id }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
      await refreshBootstrap();
    } finally {
      setBusyService(null);
    }
  };

  const handleStartAll = async () => {
    setBusyAll(true);
    try {
      const services = await api.startAllServices();
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(dict.toasts.allStarted, 'ok');
    } catch (e) {
      showToast(String(e), 'err');
      await refreshBootstrap();
    } finally {
      setBusyAll(false);
    }
  };

  const handleRestartAll = async () => {
    setBusyAll(true);
    try {
      const services = await api.restartAllServices();
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(dict.toasts.allRestarted, 'ok');
    } catch (e) {
      showToast(String(e), 'err');
      await refreshBootstrap();
    } finally {
      setBusyAll(false);
    }
  };

  const handleStopAll = async () => {
    setBusyAll(true);
    try {
      const services = await api.stopAllServices();
      setBootstrap((b) => (b ? { ...b, services } : b));
      showToast(dict.toasts.allStopped, 'info');
    } finally {
      setBusyAll(false);
    }
  };

  const handleSelectVersion = async (kind: string, id: string) => {
    const updated = await api.setActiveVersion(kind, id);
    setBootstrap(updated);
    showToast(fmt(dict.toasts.versionSelected, { kind }), 'ok');
  };

  const handleAddExternalRoot = async () => {
    const dir = await api.pickDirectory('Select External Binary Folder (e.g. C:\\AppPath)');
    if (!dir) return;
    try {
      const updated = await api.addExternalBinRoot(dir);
      setBootstrap(updated);
      showToast(fmt(dict.toasts.binaryLinked, { path: dir }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleLinkServiceBinary = async (kind: string) => {
    const dir = await api.pickDirectory(`Select ${kind} Binary Folder`);
    if (!dir) return;
    try {
      const updated = await api.linkServiceBinaryFolder(kind, dir);
      setBootstrap(updated);
      showToast(fmt(dict.toasts.binaryLinked, { path: dir }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleRemoveExternalRoot = async (path: string) => {
    const updated = await api.removeExternalBinRoot(path);
    setBootstrap(updated);
    showToast(fmt(dict.toasts.binaryUnlinked, { path }), 'info');
  };

  const handleSyncHosts = async () => {
    setSyncingHosts(true);
    try {
      const hosts_status = await api.syncHosts();
      setBootstrap((b) => (b ? { ...b, hosts_status } : b));
      showToast(dict.toasts.hostsSynced, 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setSyncingHosts(false);
    }
  };

  const handleAddLinkedSite = async () => {
    const dir = await api.pickDirectory(dict.sites.linkProject);
    if (!dir) return;
    try {
      const updated = await api.addLinkedSite(dir);
      setBootstrap(updated);
      showToast(fmt(dict.toasts.siteLinked, { path: dir }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleRemoveLinkedSite = async (path: string) => {
    try {
      const updated = await api.removeLinkedSite(path);
      setBootstrap(updated);
      showToast(fmt(dict.toasts.siteUnlinked, { path }), 'info');
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleEnableWildcardDns = async () => {
    setBusyDns(true);
    try {
      const updated = await api.enableWildcardDns();
      setBootstrap(updated);
      const tld = updated.config.tld.trim().replace(/^\./, '') || 'test';
      showToast(fmt(dict.toasts.wildcardDnsEnabled, { tld }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setBusyDns(false);
    }
  };

  const handleDisableWildcardDns = async () => {
    setBusyDns(true);
    try {
      const updated = await api.disableWildcardDns();
      setBootstrap(updated);
      showToast(dict.toasts.wildcardDnsDisabled, 'info');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setBusyDns(false);
    }
  };

  const handleTrustCa = async () => {
    setTrustingCa(true);
    try {
      const ssl_status = await api.trustRootCa();
      setBootstrap((b) => (b ? { ...b, ssl_status } : b));
      showToast(dict.toasts.caTrusted, 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setTrustingCa(false);
    }
  };

  const handleRegenerateVhosts = async () => {
    setRegenerating(true);
    try {
      const [updated] = await Promise.all([api.regenerateVhosts(), minDelay()]);
      setBootstrap(updated);
      showToast(dict.toasts.vhostsRegenerated, 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setRegenerating(false);
    }
  };

  const handleOpenAdminer = async () => {
    try {
      const url = await api.openAdminer();
      await api.openExternalUrl(url);
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleLaunchDbClient = async () => {
    try {
      await api.launchDbClient();
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleOpenTerminal = async (profile?: string, cwd?: string) => {
    try {
      await api.openTerminal(profile, cwd);
      showToast(dict.toasts.terminalOpened, 'info');
    } catch (e) {
      showToast(String(e), 'err');
    }
  };

  const handleSyncUserPath = async () => {
    setSyncingPath(true);
    try {
      const added = await api.syncUserPath();
      await refreshBootstrap();
      showToast(fmt(dict.toasts.userPathSynced, { count: added.length }), 'ok');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setSyncingPath(false);
    }
  };

  const handleClearUserPath = async () => {
    setSyncingPath(true);
    try {
      const updated = await api.clearUserPath();
      setBootstrap(updated);
      showToast(dict.toasts.userPathCleared, 'info');
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setSyncingPath(false);
    }
  };

  const handleRescan = async () => {
    setRescanning(true);
    try {
      const [updated] = await Promise.all([api.rescanAll(), minDelay(360)]);
      setBootstrap(updated);
      const totalBins = Object.values(updated.inventory.binaries).reduce(
        (sum, arr) => sum + arr.length,
        0,
      );
      showToast(
        fmt(dict.toasts.rescanDone, { bins: totalBins, sites: updated.sites.length }),
        'ok',
      );
    } finally {
      setRescanning(false);
    }
  };

  const handleInstallOnlinePackage = async (item: OnlineVersionItem) => {
    if (installingIds.includes(item.id)) return;
    setInstallingIds((prev) => [...prev, item.id]);
    setProgressMap((prev) => {
      const next = { ...prev };
      delete next[item.id];
      return next;
    });
    try {
      const updated = await api.installOnlinePackage(item);
      setBootstrap(updated);
      showToast(dict.toasts.packageInstalled, 'ok');
      setProgressMap((prev) => {
        const next = { ...prev };
        delete next[item.id];
        return next;
      });
    } catch (e) {
      showToast(String(e), 'err');
    } finally {
      setInstallingIds((prev) => prev.filter((id) => id !== item.id));
    }
  };

  const handleInstallCustomArchive = async (
    kind: string,
    versionName: string,
    source: string,
    isLocalFile: boolean,
  ): Promise<boolean> => {
    if (installingIds.includes('custom')) return false;
    setInstallingIds((prev) => [...prev, 'custom']);
    setProgressMap((prev) => {
      const next = { ...prev };
      delete next.custom;
      return next;
    });
    try {
      const updated = await api.installCustomArchive(
        kind,
        versionName,
        source,
        isLocalFile,
      );
      setBootstrap(updated);
      showToast(dict.toasts.packageInstalled, 'ok');
      setProgressMap((prev) => {
        const next = { ...prev };
        delete next.custom;
        return next;
      });
      return true;
    } catch (e) {
      showToast(String(e), 'err');
      return false;
    } finally {
      setInstallingIds((prev) => prev.filter((id) => id !== 'custom'));
    }
  };

  return (
    <I18nProvider lang={bootstrap.config.language}>
      <div
        className="h-full w-full flex flex-col overflow-hidden"
        style={{ background: 'var(--bg-app)', color: 'var(--text-primary)' }}
      >
        <Titlebar
          theme={bootstrap.config.theme}
          lang={bootstrap.config.language}
          hasUpdateDot={cachedUpdate?.status === 'update_available'}
          onToggleTheme={handleToggleTheme}
          onToggleLang={handleToggleLang}
          onOpenPalette={() => setPaletteOpen(true)}
          onOpenTerminal={() => handleOpenTerminal()}
          onOpenAbout={() => setAboutOpen(true)}
        />

        <div className="flex-1 flex min-h-0 overflow-hidden">
          <Sidebar
            activeTab={activeTab}
            onSelectTab={setActiveTab}
            services={bootstrap.services}
            sitesCount={bootstrap.sites.length}
            mailpitUnread={mailpitUnread}
            busyAll={busyAll}
            rescanning={rescanning}
            onStartAll={handleStartAll}
            onRestartAll={handleRestartAll}
            onStopAll={handleStopAll}
            onOpenAdminer={handleOpenAdminer}
            onOpenMailpit={() =>
              api.openExternalUrl(`http://127.0.0.1:${bootstrap.config.ports.mailpit_http}`)
            }
            onOpenWww={() => api.openPathInExplorer(bootstrap.www_root)}
            onRescan={handleRescan}
          />

          <main className="flex-1 min-w-0 overflow-y-auto p-5">
            {activeTab === 'overview' && (
              <OverviewView
                bootstrap={bootstrap}
                busyService={busyService}
                busyAll={busyAll}
                rescanning={rescanning}
                onSelectTab={setActiveTab}
                onStartService={handleStartService}
                onStopService={handleStopService}
                onStartAll={handleStartAll}
                onRestartAll={handleRestartAll}
                onStopAll={handleStopAll}
                onOpenTerminal={() => handleOpenTerminal()}
                onRescan={handleRescan}
                onOpenAdminer={handleOpenAdminer}
                onOpenMailpit={() =>
                  api.openExternalUrl(
                    `http://127.0.0.1:${bootstrap.config.ports.mailpit_http}`,
                  )
                }
                onOpenWww={() => api.openPathInExplorer(bootstrap.www_root)}
                onOpenUrl={(u) => api.openExternalUrl(u)}
              />
            )}

            {activeTab === 'services' && (
              <ServicesView
                bootstrap={bootstrap}
                busyService={busyService}
                rescanning={rescanning}
                onStart={handleStartService}
                onStop={handleStopService}
                onRestart={handleRestartService}
                onSelectVersion={handleSelectVersion}
                onGoPackages={() => setActiveTab('packages')}
                onLinkExternalRoot={handleAddExternalRoot}
                onLinkServiceBinary={handleLinkServiceBinary}
                onRemoveExternalRoot={handleRemoveExternalRoot}
                onRescan={handleRescan}
                onOpenAdminer={handleOpenAdminer}
                onLaunchDbClient={handleLaunchDbClient}
                onOpenPhpExtensions={() => setPhpExtOpen(true)}
                onOpenUrl={(u) => api.openExternalUrl(u)}
              />
            )}

            {activeTab === 'sites' && (
              <SitesView
                bootstrap={bootstrap}
                syncingHosts={syncingHosts}
                trustingCa={trustingCa}
                regenerating={regenerating}
                busyDns={busyDns}
                onSyncHosts={handleSyncHosts}
                onTrustCa={handleTrustCa}
                onRegenerateVhosts={handleRegenerateVhosts}
                onLinkSite={handleAddLinkedSite}
                onUnlinkSite={handleRemoveLinkedSite}
                onEnableWildcardDns={handleEnableWildcardDns}
                onDisableWildcardDns={handleDisableWildcardDns}
                onOpenWww={() => api.openPathInExplorer(bootstrap.www_root)}
                onOpenFolder={(p) => api.openPathInExplorer(p)}
                onOpenTerminal={(cwd) => handleOpenTerminal(undefined, cwd)}
                onOpenUrl={(u) => api.openExternalUrl(u)}
              />
            )}

            {activeTab === 'runtimes' && (
              <RuntimesView
                bootstrap={bootstrap}
                syncingPath={syncingPath}
                onSelectVersion={handleSelectVersion}
                onAddExternalRoot={handleAddExternalRoot}
                onRemoveExternalRoot={handleRemoveExternalRoot}
                onOpenTerminal={(prof) => handleOpenTerminal(prof)}
                onSyncUserPath={handleSyncUserPath}
                onClearUserPath={handleClearUserPath}
              />
            )}

            {activeTab === 'packages' && (
              <PackagesView
                bootstrap={bootstrap}
                installingIds={installingIds}
                progressMap={progressMap}
                onInstallPackage={handleInstallOnlinePackage}
                onInstallCustomArchive={handleInstallCustomArchive}
                onUninstalled={(updated) => {
                  setBootstrap(updated);
                  showToast(dict.toasts.packageUninstalled, 'info');
                }}
                onError={(msg) => showToast(msg, 'err')}
                onOpenBinDir={() =>
                  api.openPathInExplorer(`${bootstrap.root_dir}\\bin`)
                }
              />
            )}

            {activeTab === 'logs' && (
              <LogsView
                onOpenLogsFolder={() =>
                  api.openPathInExplorer(`${bootstrap.root_dir}\\logs`)
                }
                onNotify={showToast}
              />
            )}

            {activeTab === 'settings' && (
              <SettingsView
                bootstrap={bootstrap}
                onSaveConfig={updateConfig}
                onRegenerateCa={async () => {
                  const updated = await api.regenerateRootCa();
                  setBootstrap(updated);
                  showToast(dict.toasts.caRegenerated, 'ok');
                }}
              />
            )}
          </main>
        </div>

        <StatusBar
          bootstrap={bootstrap}
          mailpitUnread={mailpitUnread}
          onOpenAbout={() => setAboutOpen(true)}
          onGoSites={() => setActiveTab('sites')}
          onOpenMailpit={() =>
            api.openExternalUrl(`http://127.0.0.1:${bootstrap.config.ports.mailpit_http}`)
          }
        />

        {toast && (
          <div
            className="toast-enter fixed bottom-9 right-4 z-[100] px-3 py-2.5 rounded-md border text-[12px] font-mono max-w-[460px] flex items-center gap-2.5 selectable"
            style={{
              background: 'var(--bg-elevated)',
              borderColor: 'var(--border)',
              borderLeftWidth: '3px',
              borderLeftColor:
                toast.kind === 'err'
                  ? 'var(--red)'
                  : toast.kind === 'ok'
                    ? 'var(--green)'
                    : 'var(--accent)',
              color:
                toast.kind === 'err'
                  ? 'var(--red)'
                  : toast.kind === 'ok'
                    ? 'var(--green)'
                    : 'var(--text-primary)',
              boxShadow: '0 10px 28px rgba(0, 0, 0, 0.45)',
            }}
          >
            {toast.kind === 'err' ? (
              <AlertCircle size={14} className="shrink-0" />
            ) : toast.kind === 'ok' ? (
              <CheckCircle2 size={14} className="shrink-0" />
            ) : (
              <Info size={14} className="shrink-0" style={{ color: 'var(--accent)' }} />
            )}
            <span className="truncate flex-1">{toast.msg}</span>
            <button
              type="button"
              onClick={() => setToast(null)}
              className="p-0.5 rounded cursor-pointer hover:bg-[var(--bg-hover)] shrink-0"
              style={{ color: 'var(--text-dim)' }}
              aria-label="Dismiss"
            >
              <X size={12} />
            </button>
          </div>
        )}

        <CommandPalette
          open={paletteOpen}
          bootstrap={bootstrap}
          onClose={() => setPaletteOpen(false)}
          onSelectTab={setActiveTab}
          onStartAll={handleStartAll}
          onRestartAll={handleRestartAll}
          onStopAll={handleStopAll}
          onOpenTerminal={() => handleOpenTerminal()}
          onOpenAdminer={handleOpenAdminer}
          onSyncHosts={handleSyncHosts}
          onTrustCa={handleTrustCa}
          onRescan={handleRescan}
          onToggleTheme={handleToggleTheme}
          onToggleLang={handleToggleLang}
          onOpenAbout={() => setAboutOpen(true)}
          onOpenSiteUrl={(u) => api.openExternalUrl(u)}
        />

        <AboutModal
          open={aboutOpen}
          version={bootstrap.version}
          rootDir={bootstrap.root_dir}
          wwwRoot={bootstrap.www_root}
          githubRepo={bootstrap.config.github_repo}
          autoCheckUpdates={bootstrap.config.auto_check_updates}
          cachedUpdate={cachedUpdate}
          onUpdateCached={setCachedUpdate}
          onClose={() => setAboutOpen(false)}
        />

        <PhpExtensionsModal
          open={phpExtOpen}
          onClose={() => setPhpExtOpen(false)}
          onRestartServices={() => void handleRestartAll()}
          onNotify={showToast}
        />

        <ContextMenu
          bootstrap={bootstrap}
          onOpenPalette={() => setPaletteOpen(true)}
          onStartAll={handleStartAll}
          onRestartAll={handleRestartAll}
          onStopAll={handleStopAll}
          onOpenTerminal={() => handleOpenTerminal()}
          onOpenWww={() => api.openPathInExplorer(bootstrap.www_root)}
          onRescan={handleRescan}
          onToggleTheme={handleToggleTheme}
          onToggleLang={handleToggleLang}
          onOpenAbout={() => setAboutOpen(true)}
          onNotify={showToast}
        />
      </div>
    </I18nProvider>
  );
}

export default App;
