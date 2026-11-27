use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_GITHUB_REPO: &str = "aozora-kenzu/EnvStackUI";
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const ALL_BINARY_KINDS: [&str; 12] = [
    "nginx", "apache", "php", "mysql", "postgres", "redis", "mailpit", "node", "composer", "git",
    "python", "go",
];

fn default_postgres_port() -> u16 {
    5432
}

fn default_redis_port() -> u16 {
    6379
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    pub nginx_http: u16,
    pub nginx_https: u16,
    pub apache_http: u16,
    pub apache_https: u16,
    pub php_cgi: u16,
    pub mysql: u16,
    #[serde(default = "default_postgres_port")]
    pub postgres: u16,
    #[serde(default = "default_redis_port")]
    pub redis: u16,
    pub mailpit_smtp: u16,
    pub mailpit_http: u16,
}

impl Default for PortConfig {
    fn default() -> Self {
        Self {
            nginx_http: 8080,
            nginx_https: 8443,
            apache_http: 80,
            apache_https: 443,
            php_cgi: 9000,
            mysql: 3306,
            postgres: 5432,
            redis: 6379,
            mailpit_smtp: 1025,
            mailpit_http: 8025,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub language: String,
    pub theme: String,
    pub www_root: Option<String>,
    pub tld: String,
    pub auto_vhost: bool,
    pub auto_ssl: bool,
    pub minimize_to_tray: bool,
    pub start_on_launch: bool,
    pub mailpit_notifications: bool,
    pub ports: PortConfig,
    pub active_versions: HashMap<String, String>,
    pub external_bin_roots: Vec<String>,
    pub terminal_profile: String,
    pub db_client_path: Option<String>,
    pub github_repo: String,
    pub auto_check_updates: bool,
    pub user_path_entries: Vec<String>,
    pub default_site_seeded: bool,
    pub linked_sites: Vec<String>,
    pub wildcard_dns_enabled: bool,
    #[serde(default)]
    pub site_php_versions: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: "dark".to_string(),
            www_root: None,
            tld: "test".to_string(),
            auto_vhost: true,
            auto_ssl: true,
            minimize_to_tray: true,
            start_on_launch: false,
            mailpit_notifications: true,
            ports: PortConfig::default(),
            active_versions: HashMap::new(),
            external_bin_roots: Vec::new(),
            terminal_profile: "wt".to_string(),
            db_client_path: None,
            github_repo: DEFAULT_GITHUB_REPO.to_string(),
            auto_check_updates: true,
            user_path_entries: Vec::new(),
            default_site_seeded: false,
            linked_sites: Vec::new(),
            wildcard_dns_enabled: false,
            site_php_versions: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PortablePaths {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub etc: PathBuf,
    pub etc_nginx: PathBuf,
    pub etc_nginx_sites: PathBuf,
    pub etc_apache: PathBuf,
    pub etc_apache_sites: PathBuf,
    pub etc_php: PathBuf,
    pub etc_mysql: PathBuf,
    pub etc_postgres: PathBuf,
    pub etc_redis: PathBuf,
    pub etc_ssl: PathBuf,
    pub etc_ssl_certs: PathBuf,
    pub www_default: PathBuf,
    pub data: PathBuf,
    pub data_mysql: PathBuf,
    pub data_postgres: PathBuf,
    pub data_redis: PathBuf,
    pub logs: PathBuf,
    pub tmp: PathBuf,
    pub tmp_updates: PathBuf,
    pub usr: PathBuf,
    pub usr_adminer: PathBuf,
    pub config_file: PathBuf,
}

impl PortablePaths {
    pub fn resolve() -> Result<Self> {
        let root = if let Ok(custom) = std::env::var("ENVSTACK_ROOT") {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                PathBuf::from(trimmed)
            } else {
                Self::default_root()?
            }
        } else {
            Self::default_root()?
        };

        Ok(Self::from_root(root))
    }

    fn default_root() -> Result<PathBuf> {
        if cfg!(debug_assertions) {
            // In `tauri dev`, current_exe() is inside target/debug; use <repo>/.portable_dev
            let cwd = std::env::current_dir().context("Failed to read current working dir")?;
            let repo_root = if cwd.file_name().and_then(|s| s.to_str()) == Some("src-tauri") {
                cwd.parent().map(Path::to_path_buf).unwrap_or(cwd)
            } else {
                cwd
            };
            Ok(repo_root.join(".portable_dev"))
        } else {
            let exe = std::env::current_exe().context("Failed to locate current executable")?;
            Ok(exe
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")))
        }
    }

    pub fn from_root(root: PathBuf) -> Self {
        let bin = root.join("bin");
        let etc = root.join("etc");
        let etc_nginx = etc.join("nginx");
        let etc_nginx_sites = etc_nginx.join("sites-enabled");
        let etc_apache = etc.join("apache");
        let etc_apache_sites = etc_apache.join("sites-enabled");
        let etc_php = etc.join("php");
        let etc_mysql = etc.join("mysql");
        let etc_postgres = etc.join("postgres");
        let etc_redis = etc.join("redis");
        let etc_ssl = etc.join("ssl");
        let etc_ssl_certs = etc_ssl.join("certs");
        let www_default = root.join("www");
        let data = root.join("data");
        let data_mysql = data.join("mysql");
        let data_postgres = data.join("postgres");
        let data_redis = data.join("redis");
        let logs = root.join("logs");
        let tmp = root.join("tmp");
        let tmp_updates = tmp.join("updates");
        let usr = root.join("usr");
        let usr_adminer = usr.join("adminer");
        let config_file = root.join("envstack.json");

        Self {
            root,
            bin,
            etc,
            etc_nginx,
            etc_nginx_sites,
            etc_apache,
            etc_apache_sites,
            etc_php,
            etc_mysql,
            etc_postgres,
            etc_redis,
            etc_ssl,
            etc_ssl_certs,
            www_default,
            data,
            data_mysql,
            data_postgres,
            data_redis,
            logs,
            tmp,
            tmp_updates,
            usr,
            usr_adminer,
            config_file,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.bin,
            &self.etc,
            &self.etc_nginx,
            &self.etc_nginx_sites,
            &self.etc_apache,
            &self.etc_apache_sites,
            &self.etc_php,
            &self.etc_mysql,
            &self.etc_postgres,
            &self.etc_redis,
            &self.etc_ssl,
            &self.etc_ssl_certs,
            &self.www_default,
            &self.data,
            &self.data_mysql,
            &self.data_postgres,
            &self.data_redis,
            &self.logs,
            &self.tmp,
            &self.tmp_updates,
            &self.usr,
            &self.usr_adminer,
        ] {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory {}", dir.display()))?;
        }

        for kind in ALL_BINARY_KINDS {
            fs::create_dir_all(self.bin.join(kind))
                .with_context(|| format!("Failed to create bin/{kind}"))?;
        }

        Ok(())
    }

    pub fn active_www_root(&self, config: &AppConfig) -> PathBuf {
        if let Some(custom) = &config.www_root {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
        self.www_default.clone()
    }
}

pub fn load_config(paths: &PortablePaths) -> AppConfig {
    if let Ok(raw) = fs::read_to_string(&paths.config_file) {
        if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&raw) {
            if cfg.github_repo.trim().is_empty() {
                cfg.github_repo = DEFAULT_GITHUB_REPO.to_string();
            }
            if cfg.tld.trim().is_empty() {
                cfg.tld = "test".to_string();
            }
            cfg.theme = if cfg.theme.ends_with("light") {
                "light".to_string()
            } else {
                "dark".to_string()
            };
            return cfg;
        }
    }
    let cfg = AppConfig::default();
    let _ = save_config(paths, &cfg);
    cfg
}

pub fn save_config(paths: &PortablePaths, config: &AppConfig) -> Result<()> {
    let serialized = serde_json::to_string_pretty(config)?;
    fs::write(&paths.config_file, serialized)
        .with_context(|| format!("Failed to save config to {}", paths.config_file.display()))?;
    Ok(())
}

/// Normalizes a Windows path to forward slashes for Nginx, Apache, and MariaDB/MySQL configs.
pub fn slash_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    s.strip_prefix("//?/").map(|v| v.to_string()).unwrap_or(s)
}

const DEFAULT_INDEX_PHP: &str = include_str!("templates/default_index.php");

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn encode_ps_utf16le_base64(script: &str) -> String {
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base64_encode(&bytes)
}

pub fn run_elevated_powershell(ps_script: &str) -> Result<()> {
    let err_path = std::env::temp_dir().join(format!("envstack_uac_err_{}.txt", std::process::id()));
    let _ = fs::remove_file(&err_path);
    let err_esc = err_path.to_string_lossy().replace('\'', "''");

    let wrapped = format!(
        "$ErrorActionPreference = 'Stop'\r\ntry {{\r\n{ps_script}\r\n}} catch {{\r\n  [System.IO.File]::WriteAllText('{err_esc}', $_.Exception.Message)\r\n  exit 1\r\n}}"
    );
    let encoded = encode_ps_utf16le_base64(&wrapped);
    let outer_cmd = format!(
        "$p = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand','{encoded}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; if (-not $p) {{ exit 1 }}; exit $p.ExitCode"
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &outer_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("Failed to invoke UAC prompt")?;

    let captured_err = fs::read_to_string(&err_path).ok();
    let _ = fs::remove_file(&err_path);

    if !output.status.success() {
        if let Some(msg) = captured_err.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            return Err(anyhow!("{msg}"));
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow!("UAC elevation cancelled or failed"));
        }
        return Err(anyhow!("UAC elevation cancelled or failed: {stderr}"));
    }

    Ok(())
}

pub fn ensure_default_website(www_root: &Path, config: &mut AppConfig) -> Result<bool> {
    if config.default_site_seeded {
        return Ok(false);
    }

    fs::create_dir_all(www_root)
        .with_context(|| format!("Failed to create document root {}", www_root.display()))?;

    let has_existing_index = ["index.php", "index.html", "index.htm"]
        .iter()
        .any(|name| www_root.join(name).is_file());

    if !has_existing_index {
        let dest = www_root.join("index.php");
        fs::write(&dest, DEFAULT_INDEX_PHP)
            .with_context(|| format!("Failed to write default website at {}", dest.display()))?;
    }

    config.default_site_seeded = true;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_default_website_seeds_once_and_respects_deletion() {
        let base = std::env::temp_dir().join(format!("envstack_seed_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let www = base.join("www");
        let mut cfg = AppConfig::default();

        let seeded_first = ensure_default_website(&www, &mut cfg).unwrap();
        assert!(seeded_first);
        assert!(cfg.default_site_seeded);

        let index_php = www.join("index.php");
        assert!(index_php.is_file());
        let content = fs::read_to_string(&index_php).unwrap();
        assert!(content.contains("EnvStackUI"));
        let lower = content.to_lowercase();
        let has_disallowed_word = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| w == "zed" || w == "laragon");
        assert!(!has_disallowed_word);
        assert!(!lower.contains("zed.dev"));

        // Deleting index.php should not recreate it once default_site_seeded is true
        fs::remove_file(&index_php).unwrap();
        let seeded_second = ensure_default_website(&www, &mut cfg).unwrap();
        assert!(!seeded_second);
        assert!(!index_php.exists());

        // Pre-existing index.html prevents overwriting or writing index.php
        let www_html = base.join("www_existing_html");
        fs::create_dir_all(&www_html).unwrap();
        fs::write(www_html.join("index.html"), b"<h1>Custom</h1>").unwrap();
        let mut cfg_html = AppConfig::default();
        let seeded_html = ensure_default_website(&www_html, &mut cfg_html).unwrap();
        assert!(seeded_html);
        assert!(cfg_html.default_site_seeded);
        assert!(!www_html.join("index.php").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn encodes_powershell_utf16le_base64_correctly() {
        assert_eq!(encode_ps_utf16le_base64("test"), "dABlAHMAdAA=");
    }
}
