use crate::portable::{run_elevated_powershell, slash_path, AppConfig, PortablePaths};
use crate::scanner::{BinaryItem, Inventory};
use crate::ssl;
use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";
const MARKER_START: &str = "# >>> EnvStackUI Managed Hosts";
const MARKER_END: &str = "# <<< EnvStackUI Managed Hosts";
const RC_FILE_NAME: &str = ".envstackrc";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteInfo {
    pub name: String,
    pub domain: String,
    pub path: String,
    pub doc_root: String,
    pub framework: String,
    pub has_ssl: bool,
    pub is_linked: bool,
    pub http_url: String,
    pub https_url: String,
    pub php_version_id: Option<String>,
    pub php_version_label: Option<String>,
    pub php_cgi_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostsSyncStatus {
    pub in_sync: bool,
    pub needs_uac: bool,
    pub pending_add: Vec<String>,
    pub pending_remove: Vec<String>,
    pub managed_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VhostConfigs {
    pub domain: String,
    pub nginx_conf: String,
    pub apache_conf: String,
}

pub fn read_envstackrc_php(site_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(site_dir.join(RC_FILE_NAME)).ok()?;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim().eq_ignore_ascii_case("php") {
                let val = v.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

pub fn write_envstackrc_php(site_dir: &Path, version: Option<&str>) -> Result<()> {
    let rc_path = site_dir.join(RC_FILE_NAME);
    let existing = fs::read_to_string(&rc_path).unwrap_or_default();
    let mut kept: Vec<String> = existing
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if let Some((k, _)) = trimmed.split_once('=') {
                !k.trim().eq_ignore_ascii_case("php")
            } else {
                !trimmed.is_empty()
            }
        })
        .map(String::from)
        .collect();

    if let Some(ver) = version.map(str::trim).filter(|s| !s.is_empty()) {
        kept.push(format!("php={ver}"));
    }

    if kept.is_empty() {
        if rc_path.is_file() {
            let _ = fs::remove_file(&rc_path);
        }
    } else {
        let mut content = kept.join("\n");
        content.push('\n');
        fs::write(&rc_path, content)
            .with_context(|| format!("Failed to write {}", rc_path.display()))?;
    }
    Ok(())
}

pub fn find_php_binary_for_pin<'a>(
    pin: &str,
    php_bins: &'a [BinaryItem],
) -> Option<&'a BinaryItem> {
    let needle = pin.trim();
    if needle.is_empty() {
        return None;
    }
    php_bins
        .iter()
        .find(|b| b.id.eq_ignore_ascii_case(needle))
        .or_else(|| php_bins.iter().find(|b| b.version.eq_ignore_ascii_case(needle)))
        .or_else(|| php_bins.iter().find(|b| b.name.eq_ignore_ascii_case(needle)))
}

pub fn resolve_pinned_php_ports(
    config: &AppConfig,
    inventory: &Inventory,
) -> HashMap<String, u16> {
    let php_bins = inventory
        .binaries
        .get("php")
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let global_id = config.active_versions.get("php").map(String::as_str);

    let mut unique_ids: BTreeSet<String> = BTreeSet::new();
    for raw_pin in config.site_php_versions.values() {
        if let Some(bin) = find_php_binary_for_pin(raw_pin, php_bins) {
            if Some(bin.id.as_str()) != global_id {
                unique_ids.insert(bin.id.clone());
            }
        }
    }

    let mut map = HashMap::new();
    let base_port = config.ports.php_cgi;
    for (idx, bin_id) in unique_ids.into_iter().enumerate() {
        if let Some(port) = base_port.checked_add((idx as u16) + 1) {
            map.insert(bin_id, port);
        }
    }
    map
}

pub fn sync_envstackrc_pins_to_config(
    www_root: &Path,
    config: &mut AppConfig,
    inventory: &Inventory,
) -> bool {
    let php_bins = inventory
        .binaries
        .get("php")
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let tld = config.tld.trim().trim_start_matches('.');
    let tld = if tld.is_empty() { "test" } else { tld };
    let mut changed = false;

    let mut candidate_dirs: Vec<std::path::PathBuf> = Vec::new();
    if www_root.is_dir() {
        if let Ok(entries) = fs::read_dir(www_root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    candidate_dirs.push(p);
                }
            }
        }
    }
    for linked in &config.linked_sites {
        let p = Path::new(linked);
        if p.is_dir() {
            candidate_dirs.push(p.to_path_buf());
        }
    }

    for dir in candidate_dirs {
        let Some(folder_name) = dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if folder_name.starts_with('.') {
            continue;
        }
        let label = sanitize_domain_label(folder_name);
        if label.is_empty() {
            continue;
        }
        let domain_key = format!("{label}.{tld}").to_lowercase();
        if config.site_php_versions.contains_key(&domain_key) {
            continue;
        }
        if let Some(rc_ver) = read_envstackrc_php(&dir) {
            if let Some(bin) = find_php_binary_for_pin(&rc_ver, php_bins) {
                config.site_php_versions.insert(domain_key, bin.id.clone());
                changed = true;
            }
        }
    }

    changed
}

fn detect_doc_root_and_framework(project_dir: &Path) -> (std::path::PathBuf, String) {
    let public_dir = project_dir.join("public");
    let web_dir = project_dir.join("web");

    let framework = if project_dir.join("artisan").is_file() {
        "Laravel".to_string()
    } else if project_dir.join("wp-config.php").is_file()
        || project_dir.join("wp-config-sample.php").is_file()
    {
        "WordPress".to_string()
    } else if project_dir.join("composer.json").is_file() {
        "PHP / Composer".to_string()
    } else if project_dir.join("index.php").is_file() || public_dir.join("index.php").is_file() {
        "PHP".to_string()
    } else {
        "Static".to_string()
    };

    let doc_root = if public_dir.is_dir() {
        public_dir
    } else if web_dir.is_dir() {
        web_dir
    } else {
        project_dir.to_path_buf()
    };

    (doc_root, framework)
}

pub fn sanitize_domain_label(name: &str) -> String {
    let cleaned: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').to_string()
}

fn format_php_label(bin: &BinaryItem) -> String {
    match bin.variant.as_deref() {
        Some(v) if !v.is_empty() => format!("{} ({v})", bin.version),
        _ => bin.version.clone(),
    }
}

fn build_site_entry(
    path: &Path,
    tld: &str,
    ssl_dir: &Path,
    web_ports: (u16, u16),
    is_linked: bool,
    config: &AppConfig,
    php_bins: &[BinaryItem],
    pinned_ports: &HashMap<String, u16>,
) -> Option<SiteInfo> {
    if !path.is_dir() {
        return None;
    }
    let folder_name = path.file_name().and_then(|s| s.to_str())?;
    if folder_name.starts_with('.') {
        return None;
    }
    if path.join("src-tauri").join("tauri.conf.json").is_file() {
        return None;
    }

    let label = sanitize_domain_label(folder_name);
    if label.is_empty() {
        return None;
    }

    let (http_port, https_port) = web_ports;
    let domain = format!("{label}.{tld}");
    let (doc_root, framework) = detect_doc_root_and_framework(path);
    let has_ssl = ssl::site_cert_path(ssl_dir, &domain).is_file()
        && ssl::site_key_path(ssl_dir, &domain).is_file();

    let http_url = if http_port == 80 {
        format!("http://{domain}")
    } else {
        format!("http://{domain}:{http_port}")
    };
    let https_url = if https_port == 443 {
        format!("https://{domain}")
    } else {
        format!("https://{domain}:{https_port}")
    };

    let pin_raw = config
        .site_php_versions
        .get(&domain.to_lowercase())
        .cloned()
        .or_else(|| read_envstackrc_php(path));

    let pinned_bin = pin_raw
        .as_deref()
        .and_then(|raw| find_php_binary_for_pin(raw, php_bins));

    let (php_version_id, php_version_label, php_cgi_port) = if let Some(bin) = pinned_bin {
        let port = pinned_ports
            .get(&bin.id)
            .copied()
            .unwrap_or(config.ports.php_cgi);
        (Some(bin.id.clone()), Some(format_php_label(bin)), port)
    } else {
        (None, None, config.ports.php_cgi)
    };

    Some(SiteInfo {
        name: folder_name.to_string(),
        domain,
        path: path.to_string_lossy().to_string(),
        doc_root: doc_root.to_string_lossy().to_string(),
        framework,
        has_ssl,
        is_linked,
        http_url,
        https_url,
        php_version_id,
        php_version_label,
        php_cgi_port,
    })
}

pub fn scan_sites(
    www_root: &Path,
    config: &AppConfig,
    inventory: &Inventory,
    ssl_dir: &Path,
    portable_root: &Path,
    web_ports: (u16, u16),
) -> Vec<SiteInfo> {
    let tld = config.tld.trim().trim_start_matches('.');
    let tld = if tld.is_empty() { "test" } else { tld };
    let norm_root = portable_root
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase();

    let php_bins = inventory
        .binaries
        .get("php")
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let pinned_ports = resolve_pinned_php_ports(config, inventory);

    let mut sites = Vec::new();
    let mut seen_domains = HashSet::new();

    if www_root.is_dir() {
        if let Ok(entries) = fs::read_dir(www_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let norm_path = path
                    .to_string_lossy()
                    .trim_end_matches(['\\', '/'])
                    .to_lowercase();
                if !norm_root.is_empty() && norm_path == norm_root {
                    continue;
                }
                if let Some(site) = build_site_entry(
                    &path,
                    tld,
                    ssl_dir,
                    web_ports,
                    false,
                    config,
                    php_bins,
                    &pinned_ports,
                ) {
                    if seen_domains.insert(site.domain.to_lowercase()) {
                        sites.push(site);
                    }
                }
            }
        }
    }

    for linked in &config.linked_sites {
        let path = Path::new(linked);
        if let Some(site) = build_site_entry(
            path,
            tld,
            ssl_dir,
            web_ports,
            true,
            config,
            php_bins,
            &pinned_ports,
        ) {
            if seen_domains.insert(site.domain.to_lowercase()) {
                sites.push(site);
            }
        }
    }

    sites.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    sites
}

pub fn render_nginx_site_conf(
    site: &SiteInfo,
    config: &AppConfig,
    ssl_dir: &Path,
) -> String {
    let doc_root = slash_path(Path::new(&site.doc_root));
    let php_port = site.php_cgi_port;
    let http_port = config.ports.nginx_http;
    let https_port = config.ports.nginx_https;
    let domain = &site.domain;

    let mut out = format!(
        r#"server {{
    listen {http_port};
    server_name {domain} *.{domain};
    root "{doc_root}";
    index index.php index.html index.htm;
    charset utf-8;

    location / {{
        try_files $uri $uri/ /index.php?$query_string;
    }}

    location ~ \.php$ {{
        try_files $uri =404;
        fastcgi_pass 127.0.0.1:{php_port};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        include fastcgi_params;
    }}

    location ~ /\.ht {{
        deny all;
    }}
}}
"#
    );

    if config.auto_ssl {
        let cert = slash_path(&ssl::site_cert_path(ssl_dir, domain));
        let key = slash_path(&ssl::site_key_path(ssl_dir, domain));
        out.push_str(&format!(
            r#"
server {{
    listen {https_port} ssl;
    server_name {domain} *.{domain};
    root "{doc_root}";
    index index.php index.html index.htm;
    charset utf-8;

    ssl_certificate "{cert}";
    ssl_certificate_key "{key}";
    ssl_protocols TLSv1.2 TLSv1.3;

    location / {{
        try_files $uri $uri/ /index.php?$query_string;
    }}

    location ~ \.php$ {{
        try_files $uri =404;
        fastcgi_pass 127.0.0.1:{php_port};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        include fastcgi_params;
    }}

    location ~ /\.ht {{
        deny all;
    }}
}}
"#
        ));
    }

    out
}

pub fn render_apache_site_conf(
    site: &SiteInfo,
    config: &AppConfig,
    ssl_dir: &Path,
    use_fcgi: bool,
) -> String {
    let doc_root = slash_path(Path::new(&site.doc_root));
    let http_port = config.ports.apache_http;
    let https_port = config.ports.apache_https;
    let php_port = site.php_cgi_port;
    let effective_fcgi = use_fcgi || site.php_cgi_port != config.ports.php_cgi;
    let domain = &site.domain;

    let fcgi_block = if effective_fcgi {
        format!(
            r#"    ProxyFCGIBackendType GENERIC
    ProxyFCGISetEnvIf "reqenv('SCRIPT_FILENAME') =~ m#^(?:proxy:fcgi://[^/]+)?/([A-Za-z]:/.*)#" SCRIPT_FILENAME "$1"
    <FilesMatch \.php$>
        SetHandler "proxy:fcgi://127.0.0.1:{php_port}/"
    </FilesMatch>"#
        )
    } else {
        String::new()
    };

    let mut out = format!(
        r#"<VirtualHost *:{http_port}>
    DocumentRoot "{doc_root}"
    ServerName {domain}
    ServerAlias *.{domain}
    <Directory "{doc_root}">
        Options Indexes FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>
{fcgi_block}
</VirtualHost>
"#
    );

    if config.auto_ssl {
        let cert = slash_path(&ssl::site_cert_path(ssl_dir, domain));
        let key = slash_path(&ssl::site_key_path(ssl_dir, domain));
        out.push_str(&format!(
            r#"
<VirtualHost *:{https_port}>
    DocumentRoot "{doc_root}"
    ServerName {domain}
    ServerAlias *.{domain}
    SSLEngine on
    SSLCertificateFile "{cert}"
    SSLCertificateKeyFile "{key}"
    <Directory "{doc_root}">
        Options Indexes FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>
{fcgi_block}
</VirtualHost>
"#
        ));
    }

    out
}

pub fn generate_vhost_configs(
    paths: &PortablePaths,
    config: &AppConfig,
    sites: &[SiteInfo],
    apache_use_fcgi: bool,
) -> Result<()> {
    fs::create_dir_all(&paths.etc_nginx_sites)?;
    fs::create_dir_all(&paths.etc_apache_sites)?;

    let desired_files: HashSet<String> = sites
        .iter()
        .map(|s| format!("auto.{}.conf", s.domain))
        .collect();

    // Remove stale auto.*.conf
    for dir in [&paths.etc_nginx_sites, &paths.etc_apache_sites] {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("auto.")
                        && name.ends_with(".conf")
                        && !desired_files.contains(name)
                    {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    if !config.auto_vhost {
        return Ok(());
    }

    for site in sites {
        if config.auto_ssl {
            let _ = ssl::ensure_site_cert(&paths.etc_ssl, &site.domain);
        }
        let file_name = format!("auto.{}.conf", site.domain);
        let nginx_conf = render_nginx_site_conf(site, config, &paths.etc_ssl);
        let apache_conf = render_apache_site_conf(site, config, &paths.etc_ssl, apache_use_fcgi);

        fs::write(paths.etc_nginx_sites.join(&file_name), nginx_conf)?;
        fs::write(paths.etc_apache_sites.join(&file_name), apache_conf)?;
    }

    Ok(())
}

pub fn read_site_vhosts(paths: &PortablePaths, domain: &str) -> VhostConfigs {
    let file_name = format!("auto.{domain}.conf");
    let nginx_conf =
        fs::read_to_string(paths.etc_nginx_sites.join(&file_name)).unwrap_or_default();
    let apache_conf =
        fs::read_to_string(paths.etc_apache_sites.join(&file_name)).unwrap_or_default();

    VhostConfigs {
        domain: domain.to_string(),
        nginx_conf,
        apache_conf,
    }
}

fn extract_managed_domains(hosts_content: &str) -> Vec<String> {
    let mut domains = Vec::new();
    let mut in_block = false;
    for line in hosts_content.lines() {
        let trimmed = line.trim();
        if trimmed == MARKER_START {
            in_block = true;
            continue;
        }
        if trimmed == MARKER_END {
            break;
        }
        if in_block && !trimmed.is_empty() && !trimmed.starts_with('#') {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                domains.push(parts[1].to_string());
            }
        }
    }
    domains
}

pub fn merge_hosts_block(existing: &str, domains: &[String]) -> String {
    let mut kept_lines = Vec::new();
    let mut in_block = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == MARKER_START {
            in_block = true;
            continue;
        }
        if trimmed == MARKER_END {
            in_block = false;
            continue;
        }
        if !in_block {
            kept_lines.push(line.to_string());
        }
    }

    while kept_lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        kept_lines.pop();
    }

    let mut result = kept_lines.join("\r\n");
    if !domains.is_empty() {
        if !result.is_empty() {
            result.push_str("\r\n\r\n");
        }
        result.push_str(MARKER_START);
        result.push_str("\r\n");
        for d in domains {
            result.push_str(&format!("127.0.0.1 {d}\r\n"));
        }
        result.push_str(MARKER_END);
        result.push_str("\r\n");
    } else if !result.is_empty() {
        result.push_str("\r\n");
    }

    result
}

pub fn check_hosts_sync_status(sites: &[SiteInfo], tld: &str) -> HostsSyncStatus {
    let clean_tld = tld.trim().trim_start_matches('.');
    if clean_tld.eq_ignore_ascii_case("localhost") {
        return HostsSyncStatus {
            in_sync: true,
            needs_uac: false,
            pending_add: Vec::new(),
            pending_remove: Vec::new(),
            managed_domains: Vec::new(),
        };
    }

    let desired: Vec<String> = sites.iter().map(|s| s.domain.clone()).collect();
    let existing = fs::read_to_string(HOSTS_PATH).unwrap_or_default();
    let managed = extract_managed_domains(&existing);

    let desired_set: HashSet<&str> = desired.iter().map(String::as_str).collect();
    let managed_set: HashSet<&str> = managed.iter().map(String::as_str).collect();

    let pending_add: Vec<String> = desired
        .iter()
        .filter(|d| !managed_set.contains(d.as_str()))
        .cloned()
        .collect();
    let pending_remove: Vec<String> = managed
        .iter()
        .filter(|d| !desired_set.contains(d.as_str()))
        .cloned()
        .collect();

    let in_sync = pending_add.is_empty() && pending_remove.is_empty();
    HostsSyncStatus {
        in_sync,
        needs_uac: !in_sync,
        pending_add,
        pending_remove,
        managed_domains: managed,
    }
}

pub fn sync_hosts_file(paths: &PortablePaths, sites: &[SiteInfo], tld: &str) -> Result<HostsSyncStatus> {
    let status = check_hosts_sync_status(sites, tld);
    if status.in_sync {
        return Ok(status);
    }

    let existing = fs::read_to_string(HOSTS_PATH)
        .with_context(|| format!("Failed to read {HOSTS_PATH}"))?;
    let desired: Vec<String> = sites.iter().map(|s| s.domain.clone()).collect();
    let updated = merge_hosts_block(&existing, &desired);

    fs::create_dir_all(&paths.tmp)?;
    let staged = paths.tmp.join("hosts.staged");
    fs::write(&staged, updated.as_bytes())?;

    let staged_esc = staged.to_string_lossy().replace('\'', "''");
    let hosts_esc = HOSTS_PATH.replace('\'', "''");
    let ps_script = format!(
        "Copy-Item -LiteralPath '{staged_esc}' -Destination '{hosts_esc}' -Force\r\nipconfig /flushdns | Out-Null"
    );

    let res = run_elevated_powershell(&ps_script).context("Failed to update Windows hosts");
    let _ = fs::remove_file(&staged);
    res?;

    Ok(check_hosts_sync_status(sites, tld))
}

pub fn start_www_watcher<F>(
    app_handle: AppHandle,
    www_root: std::path::PathBuf,
    on_change: F,
) -> Option<RecommendedWatcher>
where
    F: Fn() + Send + Sync + 'static,
{
    let _ = fs::create_dir_all(&www_root);
    let handle = app_handle.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            use notify::EventKind;
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
            ) {
                on_change();
                let _ = handle.emit("sites-changed", ());
            }
        }
    })
    .ok()?;

    watcher.watch(&www_root, RecursiveMode::NonRecursive).ok()?;
    Some(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_inventory_with_two_phps() -> Inventory {
        let mut binaries = BTreeMap::new();
        binaries.insert(
            "php".to_string(),
            vec![
                BinaryItem {
                    id: r"C:\Env\bin\php\php-8.4.28".into(),
                    kind: "php".into(),
                    name: "php-8.4.28".into(),
                    version: "8.4.28".into(),
                    source: "portable".into(),
                    root_dir: r"C:\Env\bin\php\php-8.4.28".into(),
                    exe_path: r"C:\Env\bin\php\php-8.4.28\php.exe".into(),
                    bin_dir: r"C:\Env\bin\php\php-8.4.28".into(),
                    variant: Some("TS".into()),
                },
                BinaryItem {
                    id: r"C:\Env\bin\php\php-8.2.28".into(),
                    kind: "php".into(),
                    name: "php-8.2.28".into(),
                    version: "8.2.28".into(),
                    source: "portable".into(),
                    root_dir: r"C:\Env\bin\php\php-8.2.28".into(),
                    exe_path: r"C:\Env\bin\php\php-8.2.28\php.exe".into(),
                    bin_dir: r"C:\Env\bin\php\php-8.2.28".into(),
                    variant: Some("NTS".into()),
                },
            ],
        );
        Inventory {
            binaries,
            active_versions: HashMap::new(),
            detected_db_clients: Vec::new(),
        }
    }

    #[test]
    fn merge_hosts_block_is_idempotent() {
        let base = "127.0.0.1 localhost\r\n::1 localhost\r\n";
        let domains = vec!["blog.test".to_string(), "shop.test".to_string()];
        let once = merge_hosts_block(base, &domains);
        assert!(once.contains("127.0.0.1 blog.test"));
        assert!(once.contains("127.0.0.1 shop.test"));
        assert!(once.contains(MARKER_START));

        let twice = merge_hosts_block(&once, &domains);
        assert_eq!(once, twice);

        let cleared = merge_hosts_block(&twice, &[]);
        assert!(!cleared.contains("blog.test"));
        assert!(!cleared.contains(MARKER_START));
        assert!(cleared.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn nginx_vhost_uses_forward_slashes_and_pinned_port() {
        let mut cfg = AppConfig::default();
        cfg.active_versions
            .insert("php".into(), r"C:\Env\bin\php\php-8.4.28".into());
        cfg.site_php_versions
            .insert("legacy.test".into(), r"C:\Env\bin\php\php-8.2.28".into());
        let inv = sample_inventory_with_two_phps();
        let pinned = resolve_pinned_php_ports(&cfg, &inv);
        assert_eq!(pinned.get(r"C:\Env\bin\php\php-8.2.28").copied(), Some(9001));
        assert!(!pinned.contains_key(r"C:\Env\bin\php\php-8.4.28"));

        let unpinned_site = SiteInfo {
            name: "demo".into(),
            domain: "demo.test".into(),
            path: r"C:\DevStack\www\demo".into(),
            doc_root: r"C:\DevStack\www\demo\public".into(),
            framework: "Laravel".into(),
            has_ssl: true,
            is_linked: false,
            http_url: "http://demo.test".into(),
            https_url: "https://demo.test".into(),
            php_version_id: None,
            php_version_label: None,
            php_cgi_port: 9000,
        };
        let rendered =
            render_nginx_site_conf(&unpinned_site, &cfg, Path::new(r"C:\EnvStack\etc\ssl"));
        assert!(rendered.contains("root \"C:/DevStack/www/demo/public\";"));
        assert!(rendered.contains("fastcgi_pass 127.0.0.1:9000;"));
        assert!(!rendered.contains(r"C:\"));

        let pinned_site = SiteInfo {
            domain: "legacy.test".into(),
            php_version_id: Some(r"C:\Env\bin\php\php-8.2.28".into()),
            php_version_label: Some("8.2.28 (NTS)".into()),
            php_cgi_port: 9001,
            ..unpinned_site
        };
        let nginx_pinned =
            render_nginx_site_conf(&pinned_site, &cfg, Path::new(r"C:\EnvStack\etc\ssl"));
        assert!(nginx_pinned.contains("fastcgi_pass 127.0.0.1:9001;"));

        // Even if global Apache uses mod_php (use_fcgi = false), pinned sites proxy to their port
        let apache_pinned =
            render_apache_site_conf(&pinned_site, &cfg, Path::new(r"C:\EnvStack\etc\ssl"), false);
        assert!(apache_pinned.contains("SetHandler \"proxy:fcgi://127.0.0.1:9001/\""));
        assert!(apache_pinned.contains("ProxyFCGISetEnvIf"));
    }

    #[test]
    fn envstackrc_round_trip() {
        let base = std::env::temp_dir().join(format!("envstack_rc_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        assert_eq!(read_envstackrc_php(&base), None);
        write_envstackrc_php(&base, Some("8.2.28")).unwrap();
        assert_eq!(read_envstackrc_php(&base).as_deref(), Some("8.2.28"));

        write_envstackrc_php(&base, None).unwrap();
        assert_eq!(read_envstackrc_php(&base), None);
        assert!(!base.join(RC_FILE_NAME).exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn scan_sites_detects_envstackui_project_and_uses_web_ports() {
        let base = std::env::temp_dir().join(format!("envstack_sites_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let www = base.join("www");
        let ssl = base.join("ssl");
        fs::create_dir_all(www.join("envstackui")).unwrap();
        let linked_dir = base.join("external_client").join("client-portal");
        fs::create_dir_all(&linked_dir).unwrap();
        fs::create_dir_all(www.join("dev-repo").join("src-tauri")).unwrap();
        fs::write(www.join("dev-repo").join("src-tauri").join("tauri.conf.json"), b"{}").unwrap();
        fs::create_dir_all(&ssl).unwrap();

        let mut cfg = AppConfig::default();
        cfg.linked_sites.push(linked_dir.to_string_lossy().to_string());
        let inv = Inventory {
            binaries: BTreeMap::new(),
            active_versions: HashMap::new(),
            detected_db_clients: Vec::new(),
        };
        let apache_sites = scan_sites(&www, &cfg, &inv, &ssl, &base, (8080, 8443));
        assert_eq!(apache_sites.len(), 2);
        let env_site = apache_sites.iter().find(|s| s.name == "envstackui").unwrap();
        assert!(!env_site.is_linked);
        assert_eq!(env_site.http_url, "http://envstackui.test:8080");
        assert_eq!(env_site.https_url, "https://envstackui.test:8443");
        assert_eq!(env_site.php_cgi_port, 9000);

        let client_site = apache_sites.iter().find(|s| s.name == "client-portal").unwrap();
        assert!(client_site.is_linked);
        assert_eq!(client_site.domain, "client-portal.test");

        let nginx_sites = scan_sites(&www, &cfg, &inv, &ssl, &base, (80, 443));
        let env_nginx = nginx_sites.iter().find(|s| s.name == "envstackui").unwrap();
        assert_eq!(env_nginx.http_url, "http://envstackui.test");
        assert_eq!(env_nginx.https_url, "https://envstackui.test");

        let _ = fs::remove_dir_all(&base);
    }
}
