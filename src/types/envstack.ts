export type LanguageCode = 'en' | 'id';
export type ThemeMode = 'dark' | 'light';
export type NavTab =
  | 'overview'
  | 'services'
  | 'sites'
  | 'runtimes'
  | 'packages'
  | 'logs'
  | 'settings';

export interface PortConfig {
  nginx_http: number;
  nginx_https: number;
  apache_http: number;
  apache_https: number;
  php_cgi: number;
  mysql: number;
  mailpit_smtp: number;
  mailpit_http: number;
}

export interface AppConfig {
  language: LanguageCode;
  theme: ThemeMode;
  www_root: string | null;
  tld: string;
  auto_vhost: boolean;
  auto_ssl: boolean;
  minimize_to_tray: boolean;
  start_on_launch: boolean;
  mailpit_notifications: boolean;
  ports: PortConfig;
  active_versions: Record<string, string>;
  external_bin_roots: string[];
  terminal_profile: string;
  db_client_path: string | null;
  github_repo: string;
  auto_check_updates: boolean;
  user_path_entries: string[];
  default_site_seeded?: boolean;
  linked_sites?: string[];
  wildcard_dns_enabled?: boolean;
}

export interface BinaryItem {
  id: string;
  kind: string;
  name: string;
  version: string;
  source: 'portable' | 'external' | string;
  root_dir: string;
  exe_path: string;
  bin_dir: string;
  variant: string | null;
}

export interface ExternalDbClient {
  name: string;
  exe_path: string;
}

export interface Inventory {
  binaries: Record<string, BinaryItem[]>;
  active_versions: Record<string, string>;
  detected_db_clients: ExternalDbClient[];
}

export interface ServiceStatus {
  id: 'nginx' | 'apache' | 'php' | 'mysql' | 'mailpit' | string;
  label: string;
  state: 'stopped' | 'starting' | 'running' | 'error';
  port: number;
  secondary_port: number | null;
  pid: number | null;
  version: string | null;
  source: string | null;
  installed: boolean;
  last_error: string | null;
}

export interface SiteInfo {
  name: string;
  domain: string;
  path: string;
  doc_root: string;
  framework: string;
  has_ssl: boolean;
  is_linked: boolean;
  http_url: string;
  https_url: string;
}

export interface WildcardDnsStatus {
  enabled: boolean;
  server_running: boolean;
  nrpt_installed: boolean;
  nrpt_tld: string | null;
  matches_tld: boolean;
  listen_addr: string;
  last_error: string | null;
}

export interface PhpExtensionItem {
  name: string;
  dll_name: string;
  is_zend: boolean;
  enabled: boolean;
}

export interface PhpExtensionsPayload {
  ini_path: string;
  php_version: string;
  ext_dir: string;
  xdebug_available: boolean;
  xdebug_enabled: boolean;
  xdebug_port: number;
  extensions: PhpExtensionItem[];
}

export interface HostsSyncStatus {
  in_sync: boolean;
  needs_uac: boolean;
  pending_add: string[];
  pending_remove: string[];
  managed_domains: string[];
}

export interface SslStatus {
  ca_exists: boolean;
  ca_trusted: boolean;
  ca_cert_path: string;
  cert_count: number;
}

export interface VhostConfigs {
  domain: string;
  nginx_conf: string;
  apache_conf: string;
}

export interface AppBootstrap {
  version: string;
  root_dir: string;
  www_root: string;
  config: AppConfig;
  inventory: Inventory;
  services: ServiceStatus[];
  sites: SiteInfo[];
  hosts_status: HostsSyncStatus;
  ssl_status: SslStatus;
  dns_status: WildcardDnsStatus;
  adminer_installed: boolean;
}

export interface OnlineVersionItem {
  id: string;
  kind: string;
  name: string;
  version: string;
  variant: string | null;
  url: string;
  archive_type: string;
  target_folder: string;
  source_label: string;
  size_bytes: number | null;
  size_label: string | null;
  tags: string[];
}

export interface DownloadProgress {
  package_id: string;
  stage: 'downloading' | 'extracting' | 'done' | 'error' | string;
  downloaded: number;
  total: number | null;
  speed_bps: number;
  eta_seconds: number | null;
  message: string | null;
}

export interface LogFileInfo {
  name: string;
  size_bytes: number;
  exists: boolean;
}

export interface LogTailPayload {
  name: string;
  content: string;
  total_lines: number;
}

export interface MailpitNotification {
  id: string;
  from_name: string;
  from_address: string;
  subject: string;
  snippet: string;
  url: string;
  unread: number;
  total: number;
  theme: string;
  language: string;
}

export interface MailpitUnreadPayload {
  unread: number;
  total: number;
}

export interface UpdateCheckResult {
  status: 'up_to_date' | 'update_available' | 'no_releases' | 'error';
  current_version: string;
  latest_version: string | null;
  release_name: string | null;
  release_notes: string | null;
  release_url: string;
  asset_name: string | null;
  asset_url: string | null;
  published_at: string | null;
  error: string | null;
}
