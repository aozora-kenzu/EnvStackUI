import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type {
  AppBootstrap,
  AppConfig,
  HostsSyncStatus,
  LogFileInfo,
  LogTailPayload,
  MailpitNotification,
  OnlineVersionItem,
  PhpExtensionsPayload,
  ServiceStatus,
  SslStatus,
  UpdateCheckResult,
  VhostConfigs,
} from '../types/envstack';

export const api = {
  getBootstrap: () => invoke<AppBootstrap>('get_bootstrap'),
  rescanAll: () => invoke<AppBootstrap>('rescan_all'),
  updateConfig: (newConfig: AppConfig) =>
    invoke<AppBootstrap>('update_config', { newConfig }),
  setActiveVersion: (kind: string, versionId: string) =>
    invoke<AppBootstrap>('set_active_version', { kind, versionId }),
  addExternalBinRoot: (path: string) =>
    invoke<AppBootstrap>('add_external_bin_root', { path }),
  removeExternalBinRoot: (path: string) =>
    invoke<AppBootstrap>('remove_external_bin_root', { path }),
  linkServiceBinaryFolder: (kind: string, path: string) =>
    invoke<AppBootstrap>('link_service_binary_folder', { kind, path }),

  startService: (id: string) => invoke<ServiceStatus[]>('start_service', { id }),
  stopService: (id: string) => invoke<ServiceStatus[]>('stop_service', { id }),
  restartService: (id: string) => invoke<ServiceStatus[]>('restart_service', { id }),
  startAllServices: () => invoke<ServiceStatus[]>('start_all_services'),
  restartAllServices: () => invoke<ServiceStatus[]>('restart_all_services'),
  stopAllServices: () => invoke<ServiceStatus[]>('stop_all_services'),

  regenerateVhosts: () => invoke<AppBootstrap>('regenerate_vhosts'),
  readSiteVhosts: (domain: string) => invoke<VhostConfigs>('read_site_vhosts', { domain }),
  syncHosts: () => invoke<HostsSyncStatus>('sync_hosts'),
  trustRootCa: () => invoke<SslStatus>('trust_root_ca'),
  regenerateRootCa: () => invoke<AppBootstrap>('regenerate_root_ca'),
  addLinkedSite: (path: string) => invoke<AppBootstrap>('add_linked_site', { path }),
  removeLinkedSite: (path: string) => invoke<AppBootstrap>('remove_linked_site', { path }),
  getPhpExtensions: () => invoke<PhpExtensionsPayload>('get_php_extensions'),
  togglePhpExtension: (name: string, enabled: boolean) =>
    invoke<PhpExtensionsPayload>('toggle_php_extension', { name, enabled }),
  enableWildcardDns: () => invoke<AppBootstrap>('enable_wildcard_dns'),
  disableWildcardDns: () => invoke<AppBootstrap>('disable_wildcard_dns'),

  openTerminal: (profile?: string, cwd?: string) =>
    invoke<void>('open_terminal', { profile: profile ?? null, cwd: cwd ?? null }),
  syncUserPath: () => invoke<string[]>('sync_user_path'),
  clearUserPath: () => invoke<AppBootstrap>('clear_user_path'),

  openPathInExplorer: (path: string) => invoke<void>('open_path_in_explorer', { path }),
  openAdminer: () => invoke<string>('open_adminer'),
  launchDbClient: (exePath?: string) =>
    invoke<void>('launch_db_client', { exePath: exePath ?? null }),

  fetchOnlineVersions: (kind: string, forceRefresh = false) =>
    invoke<OnlineVersionItem[]>('fetch_online_versions', { kind, forceRefresh }),
  installOnlinePackage: (item: OnlineVersionItem) =>
    invoke<AppBootstrap>('install_online_package', { item }),
  installCustomArchive: (
    kind: string,
    versionName: string,
    source: string,
    isLocalFile: boolean,
  ) =>
    invoke<AppBootstrap>('install_custom_archive', {
      kind,
      versionName,
      source,
      isLocalFile,
    }),
  uninstallPackage: (kind: string, targetFolder: string) =>
    invoke<AppBootstrap>('uninstall_package', { kind, targetFolder }),

  listLogs: () => invoke<LogFileInfo[]>('list_logs'),
  readLogTail: (name: string, lines = 300) =>
    invoke<LogTailPayload>('read_log_tail', { name, lines }),
  clearLog: (name: string) => invoke<void>('clear_log', { name }),

  checkForUpdates: () => invoke<UpdateCheckResult>('check_for_updates'),
  downloadUpdate: (assetUrl: string, assetName: string) =>
    invoke<string>('download_update', { assetUrl, assetName }),

  getLatestMailpitNotification: () =>
    invoke<MailpitNotification | null>('get_latest_mailpit_notification'),
  hideMailpitToast: () => invoke<void>('hide_mailpit_toast'),

  openExternalUrl: (url: string) => openUrl(url),
  pickDirectory: async (title?: string) => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title,
    });
    return typeof selected === 'string' ? selected : null;
  },
  pickExecutable: async (title?: string) => {
    const selected = await openDialog({
      directory: false,
      multiple: false,
      title,
      filters: [{ name: 'Executable', extensions: ['exe', 'bat', 'cmd'] }],
    });
    return typeof selected === 'string' ? selected : null;
  },
  pickArchiveFile: async (title?: string) => {
    const selected = await openDialog({
      directory: false,
      multiple: false,
      title,
      filters: [{ name: 'Archives', extensions: ['zip', 'gz', 'tgz', 'phar'] }],
    });
    return typeof selected === 'string' ? selected : null;
  },
};
