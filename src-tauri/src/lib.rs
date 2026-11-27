mod cli;
mod dns;
mod logs;
mod mailpit;
mod mcp;
mod packages;
mod phpext;
mod portable;
mod scanner;
mod services;
mod ssl;
mod terminal;
mod updater;
mod vhosts;

pub use cli::try_handle_cli;

use dns::WildcardDnsManager;
use dns::WildcardDnsStatus;
use logs::{append_app_log, LogFileInfo, LogTailPayload};
use notify::RecommendedWatcher;
use packages::{CachedCategory, OnlineVersionItem};
use phpext::PhpExtensionsPayload;
use portable::{load_config, save_config, AppConfig, PortablePaths, APP_VERSION};
use scanner::Inventory;
use serde::Serialize;
use services::{ServiceManager, ServiceStatus};
use ssl::SslStatus;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use updater::UpdateCheckResult;
use vhosts::{HostsSyncStatus, SiteInfo, VhostConfigs};

pub struct AppState {
    pub paths: PortablePaths,
    pub config: Mutex<AppConfig>,
    pub inventory: Mutex<Inventory>,
    pub services: ServiceManager,
    pub dns: WildcardDnsManager,
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    pub online_cache: tokio::sync::Mutex<HashMap<String, CachedCategory>>,
    pub active_downloads: tokio::sync::Mutex<HashSet<String>>,
    pub latest_mailpit_notification: Mutex<Option<mailpit::MailpitNotification>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppBootstrap {
    pub version: String,
    pub root_dir: String,
    pub www_root: String,
    pub config: AppConfig,
    pub inventory: Inventory,
    pub services: Vec<ServiceStatus>,
    pub sites: Vec<SiteInfo>,
    pub hosts_status: HostsSyncStatus,
    pub ssl_status: SslStatus,
    pub dns_status: WildcardDnsStatus,
    pub adminer_installed: bool,
}

fn resolve_active_web_ports(config: &AppConfig, services: &[ServiceStatus]) -> (u16, u16) {
    let is_state = |id: &str, st: &str| services.iter().any(|s| s.id == id && s.state == st);
    let is_installed = |id: &str| services.iter().any(|s| s.id == id && s.installed);

    if is_state("apache", "running") {
        (config.ports.apache_http, config.ports.apache_https)
    } else if is_state("nginx", "running") {
        (config.ports.nginx_http, config.ports.nginx_https)
    } else if is_installed("apache") {
        (config.ports.apache_http, config.ports.apache_https)
    } else if is_installed("nginx") {
        (config.ports.nginx_http, config.ports.nginx_https)
    } else {
        (config.ports.apache_http, config.ports.apache_https)
    }
}

fn apache_uses_mod_php(inv: &Inventory, cfg: &AppConfig) -> bool {
    scanner::get_active_binary(inv, cfg, "php")
        .map(|p| {
            p.variant.as_deref() == Some("TS")
                && std::path::Path::new(&p.root_dir)
                    .join("php8apache2_4.dll")
                    .is_file()
        })
        .unwrap_or(false)
}

fn services_to_start_all(inv: &Inventory, cfg: &AppConfig) -> Vec<&'static str> {
    let has_apache = scanner::get_active_binary(inv, cfg, "apache").is_some();
    let has_nginx = scanner::get_active_binary(inv, cfg, "nginx").is_some();
    let primary_web = if has_apache {
        Some("apache")
    } else if has_nginx {
        Some("nginx")
    } else {
        None
    };

    let has_php = scanner::get_active_binary(inv, cfg, "php").is_some();
    let need_fcgi = has_php && (primary_web != Some("apache") || !apache_uses_mod_php(inv, cfg));

    let mut ids = Vec::new();
    if need_fcgi {
        ids.push("php");
    }
    if scanner::get_active_binary(inv, cfg, "mysql").is_some() {
        ids.push("mysql");
    }
    if scanner::get_active_binary(inv, cfg, "postgres").is_some() {
        ids.push("postgres");
    }
    if scanner::get_active_binary(inv, cfg, "redis").is_some() {
        ids.push("redis");
    }
    if scanner::get_active_binary(inv, cfg, "mailpit").is_some() {
        ids.push("mailpit");
    }
    if let Some(web) = primary_web {
        ids.push(web);
    }
    ids
}

fn start_all_with_clones(
    services: &ServiceManager,
    paths: &PortablePaths,
    cfg: &AppConfig,
    inv: &Inventory,
    app: &AppHandle,
) -> Vec<String> {
    let mut errors = Vec::new();
    for id in services_to_start_all(inv, cfg) {
        if scanner::get_active_binary(inv, cfg, id).is_some() {
            if let Err(e) = services.start_service(id, paths, cfg, inv, Some(app)) {
                errors.push(format!("{id}: {e:#}"));
            }
        }
    }
    errors
}

fn restart_all_with_clones(
    services: &ServiceManager,
    paths: &PortablePaths,
    cfg: &AppConfig,
    inv: &Inventory,
    app: &AppHandle,
) -> Vec<String> {
    let running_ids: HashSet<String> = services
        .get_statuses(cfg, inv)
        .into_iter()
        .filter(|s| s.state == "running")
        .map(|s| s.id)
        .collect();

    let targets: Vec<&'static str> = if running_ids.is_empty() {
        services_to_start_all(inv, cfg)
    } else {
        ["php", "mysql", "postgres", "redis", "mailpit", "apache", "nginx"]
            .into_iter()
            .filter(|id| running_ids.contains(*id) && scanner::get_active_binary(inv, cfg, id).is_some())
            .collect()
    };

    services.stop_all(paths, Some(app));
    std::thread::sleep(std::time::Duration::from_millis(250));

    let mut errors = Vec::new();
    for id in targets {
        if let Err(e) = services.start_service(id, paths, cfg, inv, Some(app)) {
            errors.push(format!("{id}: {e:#}"));
        }
    }
    errors
}

fn sync_vhosts_with_clones(
    paths: &PortablePaths,
    cfg: &AppConfig,
    inv: &Inventory,
    services: &ServiceManager,
) -> anyhow::Result<()> {
    let statuses = services.get_statuses(cfg, inv);
    let web_ports = resolve_active_web_ports(cfg, &statuses);
    let www_root = paths.active_www_root(cfg);
    let sites = vhosts::scan_sites(&www_root, cfg, inv, &paths.etc_ssl, &paths.root, web_ports);
    let apache_fcgi = !apache_uses_mod_php(inv, cfg);
    vhosts::generate_vhost_configs(paths, cfg, &sites, apache_fcgi)
}

fn spawn_www_watcher_for_app(
    app: &AppHandle,
    www_root: PathBuf,
) -> Option<RecommendedWatcher> {
    let watch_app = app.clone();
    vhosts::start_www_watcher(app.clone(), www_root, move || {
        let state = watch_app.state::<AppState>();
        let cfg = state.config.lock().unwrap().clone();
        if !cfg.auto_vhost {
            return;
        }
        let inv = state.inventory.lock().unwrap().clone();
        let _ = sync_vhosts_with_clones(&state.paths, &cfg, &inv, &state.services);
    })
}

fn build_bootstrap(state: &AppState) -> AppBootstrap {
    let config = state.config.lock().unwrap().clone();
    let inventory = state.inventory.lock().unwrap().clone();
    let services = state.services.get_statuses(&config, &inventory);
    let web_ports = resolve_active_web_ports(&config, &services);
    let www_root = state.paths.active_www_root(&config);
    let sites = vhosts::scan_sites(
        &www_root,
        &config,
        &inventory,
        &state.paths.etc_ssl,
        &state.paths.root,
        web_ports,
    );
    let hosts_status = vhosts::check_hosts_sync_status(&sites, &config.tld);
    let ssl_status = ssl::get_ssl_status(&state.paths.etc_ssl);
    let dns_status = state
        .dns
        .get_status(config.wildcard_dns_enabled, &config.tld);
    let adminer_installed = state.paths.usr_adminer.join("index.php").is_file();

    AppBootstrap {
        version: APP_VERSION.to_string(),
        root_dir: state.paths.root.to_string_lossy().to_string(),
        www_root: www_root.to_string_lossy().to_string(),
        config,
        inventory,
        services,
        sites,
        hosts_status,
        ssl_status,
        dns_status,
        adminer_installed,
    }
}

#[tauri::command]
fn get_bootstrap(state: State<'_, AppState>) -> AppBootstrap {
    build_bootstrap(&state)
}

#[tauri::command]
async fn rescan_all(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let paths = state.paths.clone();
    let mut cfg = state.config.lock().unwrap().clone();
    let (cfg, inv) = tokio::task::spawn_blocking(move || {
        let (inv, changed) = scanner::scan_inventory(&paths, &mut cfg);
        if changed {
            let _ = save_config(&paths, &cfg);
        }
        (cfg, inv)
    })
    .await
    .map_err(|e| e.to_string())?;

    *state.config.lock().unwrap() = cfg;
    *state.inventory.lock().unwrap() = inv;
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn update_config(
    new_config: AppConfig,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppBootstrap, String> {
    let old_www = {
        let cfg = state.config.lock().unwrap();
        state.paths.active_www_root(&cfg)
    };
    let mut updated = new_config;
    let (inv, _) = scanner::scan_inventory(&state.paths, &mut updated);
    save_config(&state.paths, &updated).map_err(|e| e.to_string())?;
    state.dns.set_tld(&updated.tld);

    let new_www = state.paths.active_www_root(&updated);
    if new_www != old_www {
        let new_watcher = spawn_www_watcher_for_app(&app, new_www);
        *state.watcher.lock().unwrap() = new_watcher;
    }

    *state.config.lock().unwrap() = updated;
    *state.inventory.lock().unwrap() = inv;
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn set_active_version(
    kind: String,
    version_id: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.active_versions.insert(kind, version_id);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn add_external_bin_root(
    path: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".into());
    }
    let mut cfg = state.config.lock().unwrap();
    if !cfg
        .external_bin_roots
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&trimmed))
    {
        cfg.external_bin_roots.push(trimmed);
    }
    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    *state.inventory.lock().unwrap() = inv;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn remove_external_bin_root(
    path: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.external_bin_roots
        .retain(|p| !p.eq_ignore_ascii_case(&path));
    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    *state.inventory.lock().unwrap() = inv;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn link_service_binary_folder(
    kind: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".into());
    }

    let mut cfg = state.config.lock().unwrap();
    let already_present = cfg
        .external_bin_roots
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&trimmed));

    if !already_present {
        cfg.external_bin_roots.push(trimmed.clone());
    }

    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    let norm_target = trimmed.trim_end_matches(['\\', '/']).to_lowercase();
    let matched = inv.binaries.get(&kind).and_then(|items| {
        items.iter().find(|item| {
            let norm_root = item.root_dir.trim_end_matches(['\\', '/']).to_lowercase();
            norm_root == norm_target || norm_root.starts_with(&format!("{norm_target}\\"))
        })
    });

    if let Some(bin) = matched {
        cfg.active_versions.insert(kind, bin.id.clone());
        save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
        *state.inventory.lock().unwrap() = inv;
        drop(cfg);
        Ok(build_bootstrap(&state))
    } else {
        if !already_present {
            cfg.external_bin_roots
                .retain(|p| !p.eq_ignore_ascii_case(&trimmed));
        }
        Err(format!("No '{kind}' binary found in {trimmed}"))
    }
}

#[tauri::command]
async fn start_service(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        services
            .start_service(&id, &paths, &cfg, &inv, Some(&app))
            .map_err(|e| format!("{e:#}"))?;
        Ok::<Vec<ServiceStatus>, String>(services.get_statuses(&cfg, &inv))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_service(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        services
            .stop_service(&id, &paths, Some(&app))
            .map_err(|e| format!("{e:#}"))?;
        Ok::<Vec<ServiceStatus>, String>(services.get_statuses(&cfg, &inv))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restart_service(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        let _ = services.stop_service(&id, &paths, Some(&app));
        services
            .start_service(&id, &paths, &cfg, &inv, Some(&app))
            .map_err(|e| format!("{e:#}"))?;
        Ok::<Vec<ServiceStatus>, String>(services.get_statuses(&cfg, &inv))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_all_services(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        let errors = start_all_with_clones(&services, &paths, &cfg, &inv, &app);
        if !errors.is_empty() {
            return Err(errors.join(" | "));
        }
        Ok(services.get_statuses(&cfg, &inv))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restart_all_services(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        let errors = restart_all_with_clones(&services, &paths, &cfg, &inv, &app);
        if !errors.is_empty() {
            return Err(errors.join(" | "));
        }
        Ok(services.get_statuses(&cfg, &inv))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_all_services(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ServiceStatus>, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        services.stop_all(&paths, Some(&app));
        services.get_statuses(&cfg, &inv)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn regenerate_vhosts(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let services = state.services.clone();

    tokio::task::spawn_blocking(move || {
        sync_vhosts_with_clones(&paths, &cfg, &inv, &services).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn read_site_vhosts(domain: String, state: State<'_, AppState>) -> VhostConfigs {
    vhosts::read_site_vhosts(&state.paths, &domain)
}

#[tauri::command]
async fn sync_hosts(state: State<'_, AppState>) -> Result<HostsSyncStatus, String> {
    let paths = state.paths.clone();
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let statuses = state.services.get_statuses(&cfg, &inv);
    let web_ports = resolve_active_web_ports(&cfg, &statuses);

    tokio::task::spawn_blocking(move || {
        let www_root = paths.active_www_root(&cfg);
        let sites =
            vhosts::scan_sites(&www_root, &cfg, &inv, &paths.etc_ssl, &paths.root, web_ports);
        vhosts::sync_hosts_file(&paths, &sites, &cfg.tld).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn trust_root_ca(state: State<'_, AppState>) -> Result<SslStatus, String> {
    let ssl_dir = state.paths.etc_ssl.clone();
    tokio::task::spawn_blocking(move || {
        ssl::trust_root_ca(&ssl_dir).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn regenerate_root_ca(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let ssl_dir = state.paths.etc_ssl.clone();
    tokio::task::spawn_blocking(move || {
        ssl::regenerate_root_ca(&ssl_dir).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn add_linked_site(path: String, state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let trimmed = path.trim().trim_end_matches(['\\', '/']).to_string();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".into());
    }
    let dir_path = PathBuf::from(&trimmed);
    if !dir_path.is_dir() {
        return Err(format!("Folder does not exist: {trimmed}"));
    }

    let (cfg, inv) = {
        let mut cfg = state.config.lock().unwrap();
        if !cfg
            .linked_sites
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&trimmed))
        {
            cfg.linked_sites.push(trimmed);
            save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
        }
        (cfg.clone(), state.inventory.lock().unwrap().clone())
    };

    if cfg.auto_vhost {
        let paths = state.paths.clone();
        let services = state.services.clone();
        tokio::task::spawn_blocking(move || {
            sync_vhosts_with_clones(&paths, &cfg, &inv, &services).map_err(|e| format!("{e:#}"))
        })
        .await
        .map_err(|e| e.to_string())??;
    }

    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn remove_linked_site(
    path: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let (cfg, inv) = {
        let mut cfg = state.config.lock().unwrap();
        cfg.linked_sites.retain(|p| !p.eq_ignore_ascii_case(&path));
        save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
        (cfg.clone(), state.inventory.lock().unwrap().clone())
    };

    let paths = state.paths.clone();
    let services = state.services.clone();
    tokio::task::spawn_blocking(move || {
        sync_vhosts_with_clones(&paths, &cfg, &inv, &services).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn set_site_php_version(
    domain: String,
    php_version_id: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppBootstrap, String> {
    let domain_key = domain.trim().to_lowercase();
    let pin_val = php_version_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let (cfg, inv) = {
        let mut cfg = state.config.lock().unwrap();
        let inv = state.inventory.lock().unwrap().clone();
        let php_bins = inv.binaries.get("php").map(Vec::as_slice).unwrap_or(&[]);

        let global_php = cfg.active_versions.get("php").cloned();
        let resolved_bin = pin_val
            .as_deref()
            .and_then(|v| vhosts::find_php_binary_for_pin(v, php_bins))
            .cloned();

        if let Some(ref bin) = resolved_bin {
            if Some(&bin.id) == global_php.as_ref() {
                cfg.site_php_versions.remove(&domain_key);
            } else {
                cfg.site_php_versions.insert(domain_key.clone(), bin.id.clone());
            }
        } else if pin_val.is_some() {
            return Err("Selected PHP version was not found in inventory".into());
        } else {
            cfg.site_php_versions.remove(&domain_key);
        }

        save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;

        // Persist .envstackrc inside the matching site folder
        let www_root = state.paths.active_www_root(&cfg);
        let statuses = state.services.get_statuses(&cfg, &inv);
        let web_ports = resolve_active_web_ports(&cfg, &statuses);
        let sites = vhosts::scan_sites(
            &www_root,
            &cfg,
            &inv,
            &state.paths.etc_ssl,
            &state.paths.root,
            web_ports,
        );
        if let Some(site) = sites.iter().find(|s| s.domain.eq_ignore_ascii_case(&domain_key)) {
            let rc_ver = resolved_bin
                .as_ref()
                .filter(|b| Some(&b.id) != global_php.as_ref())
                .map(|b| b.version.as_str());
            vhosts::write_envstackrc_php(std::path::Path::new(&site.path), rc_ver)
                .map_err(|e| format!("{e:#}"))?;
        }

        (cfg.clone(), inv)
    };

    let paths = state.paths.clone();
    let services = state.services.clone();
    tokio::task::spawn_blocking(move || {
        sync_vhosts_with_clones(&paths, &cfg, &inv, &services).map_err(|e| format!("{e:#}"))?;
        let statuses = services.get_statuses(&cfg, &inv);
        let web_or_php_running = statuses
            .iter()
            .any(|s| matches!(s.id.as_str(), "apache" | "nginx" | "php") && s.state == "running");
        if web_or_php_running {
            services
                .sync_pinned_php_workers(&paths, &cfg, &inv, Some(&app))
                .map_err(|e| format!("{e:#}"))?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn list_databases(engine: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    tokio::task::spawn_blocking(move || match engine.as_str() {
        "mysql" => services::list_mysql_databases(&cfg, &inv).map_err(|e| format!("{e:#}")),
        "postgres" => services::list_postgres_databases(&cfg, &inv).map_err(|e| format!("{e:#}")),
        other => Err(format!("Unsupported database engine: {other}")),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn create_database(
    engine: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    tokio::task::spawn_blocking(move || match engine.as_str() {
        "mysql" => services::create_mysql_database(&name, &cfg, &inv).map_err(|e| format!("{e:#}")),
        "postgres" => {
            services::create_postgres_database(&name, &cfg, &inv).map_err(|e| format!("{e:#}"))
        }
        other => Err(format!("Unsupported database engine: {other}")),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_php_extensions(state: State<'_, AppState>) -> Result<PhpExtensionsPayload, String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    phpext::list_php_extensions(&state.paths, &cfg, &inv).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn toggle_php_extension(
    name: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<PhpExtensionsPayload, String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    phpext::set_php_extension_enabled(&state.paths, &cfg, &inv, &name, enabled)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn enable_wildcard_dns(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let tld = { state.config.lock().unwrap().tld.clone() };
    state
        .dns
        .start(&tld)
        .map_err(|e| format!("{e:#}"))?;

    let tld_clone = tld.clone();
    if let Err(err) = tokio::task::spawn_blocking(move || dns::install_nrpt_rule(&tld_clone))
        .await
        .map_err(|e| e.to_string())?
    {
        state.dns.stop();
        return Err(format!("{err:#}"));
    }

    {
        let mut cfg = state.config.lock().unwrap();
        cfg.wildcard_dns_enabled = true;
        save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    }

    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn disable_wildcard_dns(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    tokio::task::spawn_blocking(dns::remove_nrpt_rule)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;

    state.dns.stop();
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.wildcard_dns_enabled = false;
        save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    }

    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn open_terminal(
    profile: Option<String>,
    cwd: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let selected_profile = profile.unwrap_or_else(|| cfg.terminal_profile.clone());
    let work_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| state.paths.active_www_root(&cfg));

    terminal::open_terminal(&selected_profile, &work_dir, &cfg, &inv).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn sync_user_path(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap().clone();
    let added = terminal::sync_user_path(&mut cfg, &inv).map_err(|e| format!("{e:#}"))?;
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    Ok(added)
}

#[tauri::command]
fn clear_user_path(state: State<'_, AppState>) -> Result<AppBootstrap, String> {
    let mut cfg = state.config.lock().unwrap();
    terminal::clear_user_path(&mut cfg).map_err(|e| format!("{e:#}"))?;
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn open_path_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {path}"));
    }
    Command::new("explorer.exe")
        .arg(&p)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn open_adminer(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    packages::ensure_adminer(&state.paths, Some(&app))
        .await
        .map_err(|e| format!("{e:#}"))?;

    let (cfg, inv) = {
        let c = state.config.lock().unwrap().clone();
        let i = state.inventory.lock().unwrap().clone();
        (c, i)
    };
    let statuses = state.services.get_statuses(&cfg, &inv);
    let nginx_running = statuses.iter().any(|s| s.id == "nginx" && s.state == "running");
    let apache_running = statuses
        .iter()
        .any(|s| s.id == "apache" && s.state == "running");

    let url = if nginx_running && !apache_running {
        format!("http://localhost:{}/__adminer/", cfg.ports.nginx_http)
    } else {
        format!("http://localhost:{}/__adminer/", cfg.ports.apache_http)
    };
    Ok(url)
}

#[tauri::command]
fn launch_db_client(exe_path: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.config.lock().unwrap().clone();
    let inv = state.inventory.lock().unwrap().clone();
    let target = exe_path
        .or(cfg.db_client_path)
        .or_else(|| inv.detected_db_clients.first().map(|c| c.exe_path.clone()))
        .ok_or_else(|| "No external database client configured or detected".to_string())?;

    Command::new(&target)
        .spawn()
        .map_err(|e| format!("Failed to launch {target}: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn fetch_online_versions(
    kind: String,
    force_refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<OnlineVersionItem>, String> {
    let refresh = force_refresh.unwrap_or(false);
    let now = chrono::Utc::now().timestamp();
    if !refresh {
        let cache = state.online_cache.lock().await;
        if let Some(entry) = cache.get(&kind) {
            if now - entry.fetched_at < packages::CACHE_TTL_SECS {
                return Ok(entry.items.clone());
            }
        }
    }

    let items = packages::fetch_online_versions(&kind)
        .await
        .map_err(|e| format!("{e:#}"))?;
    {
        let mut cache = state.online_cache.lock().await;
        cache.insert(
            kind,
            CachedCategory {
                fetched_at: now,
                items: items.clone(),
            },
        );
        let _ = packages::save_online_cache(&state.paths, &cache);
    }
    Ok(items)
}

#[tauri::command]
async fn install_online_package(
    item: OnlineVersionItem,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppBootstrap, String> {
    let lock_key = item.id.clone();
    {
        let mut active = state.active_downloads.lock().await;
        if !active.insert(lock_key.clone()) {
            return Err(format!("Already downloading '{lock_key}'"));
        }
    }

    let paths = state.paths.clone();
    let res = packages::install_online_package(&paths, &item, Some(&app)).await;
    state.active_downloads.lock().await.remove(&lock_key);
    res.map_err(|e| format!("{e:#}"))?;

    let mut cfg = state.config.lock().unwrap();
    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    *state.inventory.lock().unwrap() = inv;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
async fn install_custom_archive(
    kind: String,
    version_name: String,
    source: String,
    is_local_file: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppBootstrap, String> {
    let lock_key = "custom".to_string();
    {
        let mut active = state.active_downloads.lock().await;
        if !active.insert(lock_key.clone()) {
            return Err("Already installing a custom package".into());
        }
    }

    let paths = state.paths.clone();
    let res = packages::install_custom_archive(
        &paths,
        &kind,
        &version_name,
        &source,
        is_local_file,
        Some(&app),
    )
    .await;
    state.active_downloads.lock().await.remove(&lock_key);
    res.map_err(|e| format!("{e:#}"))?;

    let mut cfg = state.config.lock().unwrap();
    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    *state.inventory.lock().unwrap() = inv;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn uninstall_package(
    kind: String,
    target_folder: String,
    state: State<'_, AppState>,
) -> Result<AppBootstrap, String> {
    let mut cfg = state.config.lock().unwrap();
    let current_inv = state.inventory.lock().unwrap().clone();

    if kind != "adminer" {
        let statuses = state.services.get_statuses(&cfg, &current_inv);
        let running_match = statuses.iter().find(|svc| {
            if svc.state != "running" && svc.state != "starting" {
                return false;
            }
            if svc.id != kind {
                return false;
            }
            scanner::get_active_binary(&current_inv, &cfg, &svc.id)
                .map(|b| {
                    b.source == "portable"
                        && b.root_dir
                            .to_lowercase()
                            .contains(&target_folder.to_lowercase())
                })
                .unwrap_or(false)
        });
        if let Some(svc) = running_match {
            return Err(format!(
                "Stop '{}' service before uninstalling this package.",
                svc.label
            ));
        }
    }

    packages::uninstall_package(&state.paths, &kind, &target_folder)
        .map_err(|e| format!("{e:#}"))?;

    let (inv, _) = scanner::scan_inventory(&state.paths, &mut cfg);
    save_config(&state.paths, &cfg).map_err(|e| e.to_string())?;
    *state.inventory.lock().unwrap() = inv;
    drop(cfg);
    Ok(build_bootstrap(&state))
}

#[tauri::command]
fn list_logs(state: State<'_, AppState>) -> Vec<LogFileInfo> {
    logs::list_logs(&state.paths.logs)
}

#[tauri::command]
fn read_log_tail(name: String, lines: usize, state: State<'_, AppState>) -> LogTailPayload {
    logs::read_log_tail(&state.paths.logs, &name, lines)
}

#[tauri::command]
fn clear_log(name: String, state: State<'_, AppState>) -> Result<(), String> {
    logs::clear_log(&state.paths.logs, &name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_for_updates(state: State<'_, AppState>) -> Result<UpdateCheckResult, String> {
    let repo = state.config.lock().unwrap().github_repo.clone();
    Ok(updater::check_for_updates(&repo).await)
}

#[tauri::command]
async fn install_and_restart_update(
    asset_url: String,
    asset_name: String,
    checksum_url: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("Self-update is only available in portable release builds.".into());
    }

    let paths = state.paths.clone();
    let staged_exe = updater::download_and_stage_update(
        &paths,
        &asset_url,
        &asset_name,
        checksum_url.as_deref(),
        Some(&app),
    )
    .await
    .map_err(|e| format!("{e:#}"))?;

    let _ = app.emit(
        "update-download-progress",
        serde_json::json!({
            "stage": "installing",
            "downloaded": 100,
            "total": 100,
            "speed_bps": 0,
        }),
    );

    state.dns.stop();
    let services = state.services.clone();
    let paths_stop = paths.clone();
    let app_stop = app.clone();
    tokio::task::spawn_blocking(move || {
        services.stop_all(&paths_stop, Some(&app_stop));
    })
    .await
    .map_err(|e| e.to_string())?;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let old_exe = updater::swap_executable_files(&current_exe, &staged_exe)
        .map_err(|e| format!("{e:#}"))?;

    let spawn_res = Command::new(&current_exe)
        .args(["--wait-pid", &std::process::id().to_string()])
        .spawn();

    if let Err(err) = spawn_res {
        updater::rollback_executable_swap(&current_exe, &old_exe);
        return Err(format!("Failed to launch updated binary: {err}"));
    }

    let app_exit = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        app_exit.exit(0);
    });

    Ok(())
}

#[tauri::command]
fn get_latest_mailpit_notification(
    state: State<'_, AppState>,
) -> Option<mailpit::MailpitNotification> {
    state.latest_mailpit_notification.lock().unwrap().clone()
}

#[tauri::command]
fn hide_mailpit_toast(app: AppHandle) {
    if let Some(win) = app.get_webview_window("mailpit-toast") {
        let _ = win.hide();
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "tray_show", "Show EnvStackUI", true, None::<&str>)?;
    let start_all = MenuItem::with_id(app, "tray_start_all", "Start All Services", true, None::<&str>)?;
    let restart_all = MenuItem::with_id(app, "tray_restart_all", "Restart All Services", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "tray_stop_all", "Stop All Services", true, None::<&str>)?;
    let terminal_item = MenuItem::with_id(app, "tray_terminal", "Open Terminal", true, None::<&str>)?;
    let root_item = MenuItem::with_id(app, "tray_www", "Open www Root", true, None::<&str>)?;
    let about_item = MenuItem::with_id(app, "tray_about", "About / Check Updates", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "Quit EnvStackUI", true, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &sep1,
            &start_all,
            &restart_all,
            &stop_all,
            &terminal_item,
            &root_item,
            &sep2,
            &about_item,
            &quit_item,
        ],
    )?;

    let icon_bytes = include_bytes!("../icons/tray.png");
    let icon = tauri::image::Image::from_bytes(icon_bytes)?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("EnvStackUI")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let state = app.state::<AppState>();
            match event.id.as_ref() {
                "tray_show" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "tray_start_all" => {
                    let cfg = state.config.lock().unwrap().clone();
                    let inv = state.inventory.lock().unwrap().clone();
                    let _ = start_all_with_clones(&state.services, &state.paths, &cfg, &inv, app);
                }
                "tray_restart_all" => {
                    let cfg = state.config.lock().unwrap().clone();
                    let inv = state.inventory.lock().unwrap().clone();
                    let _ = restart_all_with_clones(&state.services, &state.paths, &cfg, &inv, app);
                }
                "tray_stop_all" => {
                    state.services.stop_all(&state.paths, Some(app));
                }
                "tray_terminal" => {
                    let cfg = state.config.lock().unwrap().clone();
                    let inv = state.inventory.lock().unwrap().clone();
                    let www = state.paths.active_www_root(&cfg);
                    let _ = terminal::open_terminal(&cfg.terminal_profile, &www, &cfg, &inv);
                }
                "tray_www" => {
                    let cfg = state.config.lock().unwrap().clone();
                    let www = state.paths.active_www_root(&cfg);
                    let _ = Command::new("explorer.exe").arg(www).spawn();
                }
                "tray_about" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                    let _ = app.emit("open-about", ());
                }
                "tray_quit" => {
                    state.dns.stop();
                    state.services.stop_all(&state.paths, Some(app));
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

pub fn handle_prelaunch_wait() {
    updater::handle_prelaunch_wait();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let paths =
                PortablePaths::resolve().expect("Failed to resolve EnvStackUI portable paths");
            paths
                .ensure_dirs()
                .expect("Failed to create portable directories");
            updater::cleanup_old_update_files(&paths);
            services::ServiceManager::cleanup_orphan_pids(&paths);
            let _ = ssl::ensure_root_ca(&paths.etc_ssl);

            let mut config = load_config(&paths);
            let initial_www = paths.active_www_root(&config);
            let seeded = portable::ensure_default_website(&initial_www, &mut config).unwrap_or(false);
            let (inventory, changed) = scanner::scan_inventory(&paths, &mut config);
            let rc_changed = vhosts::sync_envstackrc_pins_to_config(&initial_www, &mut config, &inventory);
            if changed || seeded || rc_changed {
                let _ = save_config(&paths, &config);
            }

            append_app_log(
                &paths.logs,
                None,
                "INFO",
                &format!(
                    "EnvStackUI v{} initialized at {}",
                    APP_VERSION,
                    paths.root.display()
                ),
            );

            let www_root = paths.active_www_root(&config);
            let initial_online_cache = packages::load_online_cache(&paths);
            let dns_mgr = WildcardDnsManager::new(&config.tld);
            if config.wildcard_dns_enabled
                || dns_mgr
                    .get_status(config.wildcard_dns_enabled, &config.tld)
                    .nrpt_installed
            {
                let _ = dns_mgr.start(&config.tld);
            }

            let app_state = AppState {
                paths: paths.clone(),
                config: Mutex::new(config.clone()),
                inventory: Mutex::new(inventory),
                services: ServiceManager::new(),
                dns: dns_mgr,
                watcher: Mutex::new(None),
                online_cache: tokio::sync::Mutex::new(initial_online_cache),
                active_downloads: tokio::sync::Mutex::new(HashSet::new()),
                latest_mailpit_notification: Mutex::new(None),
            };
            app.manage(app_state);

            setup_tray(app.handle())?;
            mailpit::start_mailpit_watcher(app.handle().clone());

            let watcher = spawn_www_watcher_for_app(app.handle(), www_root);
            let state = app.state::<AppState>();
            *state.watcher.lock().unwrap() = watcher;

            if config.start_on_launch {
                let paths_bg = paths.clone();
                let cfg_bg = state.config.lock().unwrap().clone();
                let inv_bg = state.inventory.lock().unwrap().clone();
                let svc_bg = state.services.clone();
                let app_bg = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = start_all_with_clones(&svc_bg, &paths_bg, &cfg_bg, &inv_bg, &app_bg);
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                let state = window.state::<AppState>();
                let minimize = state.config.lock().unwrap().minimize_to_tray;
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    state.dns.stop();
                    state
                        .services
                        .stop_all(&state.paths, Some(window.app_handle()));
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            rescan_all,
            update_config,
            set_active_version,
            add_external_bin_root,
            remove_external_bin_root,
            link_service_binary_folder,
            start_service,
            stop_service,
            restart_service,
            start_all_services,
            restart_all_services,
            stop_all_services,
            regenerate_vhosts,
            read_site_vhosts,
            sync_hosts,
            trust_root_ca,
            regenerate_root_ca,
            add_linked_site,
            remove_linked_site,
            set_site_php_version,
            list_databases,
            create_database,
            get_php_extensions,
            toggle_php_extension,
            enable_wildcard_dns,
            disable_wildcard_dns,
            open_terminal,
            sync_user_path,
            clear_user_path,
            open_path_in_explorer,
            open_adminer,
            launch_db_client,
            fetch_online_versions,
            install_online_package,
            install_custom_archive,
            uninstall_package,
            list_logs,
            read_log_tail,
            clear_log,
            check_for_updates,
            install_and_restart_update,
            get_latest_mailpit_notification,
            hide_mailpit_toast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running EnvStackUI");
}
