use crate::logs::append_app_log;
use crate::portable::{slash_path, AppConfig, PortablePaths, CREATE_NO_WINDOW};
use crate::scanner::{get_active_binary, BinaryItem, Inventory};
use crate::vhosts;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub const ALL_SERVICES: [&str; 5] = ["apache", "nginx", "php", "mysql", "mailpit"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub id: String,
    pub label: String,
    pub state: String, // "stopped" | "starting" | "running" | "error"
    pub port: u16,
    pub secondary_port: Option<u16>,
    pub pid: Option<u32>,
    pub version: Option<String>,
    pub source: Option<String>,
    pub installed: bool,
    pub last_error: Option<String>,
}

struct RunningProc {
    child: Option<Child>,
    pid: u32,
    desired_running: bool,
    exe_path: Option<PathBuf>,
    port: u16,
}

#[derive(Clone, Default)]
pub struct ServiceManager {
    procs: Arc<Mutex<HashMap<String, RunningProc>>>,
    errors: Arc<Mutex<HashMap<String, String>>>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_port_free(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    pub fn describe_port_holder(port: u16) -> Option<String> {
        let netstat = Command::new("netstat")
            .args(["-ano", "-p", "TCP"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&netstat.stdout);
        let needle = format!(":{port}");

        let mut pid_found: Option<u32> = None;
        for line in stdout.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 5 && cols[3].eq_ignore_ascii_case("LISTENING") {
                if let Some(addr_port) = cols[1].rsplit(':').next() {
                    if addr_port == port.to_string() && cols[1].contains(&needle) {
                        if let Ok(pid) = cols[4].parse::<u32>() {
                            pid_found = Some(pid);
                            break;
                        }
                    }
                }
            }
        }

        let pid = pid_found?;
        if pid == 4 {
            return Some("System (IIS / HTTP.sys) (PID 4)".to_string());
        }

        let proc_name = image_name_for_pid(pid).unwrap_or_else(|| "another process".to_string());
        Some(format!("{proc_name} (PID {pid})"))
    }

    fn port_in_use_error(port: u16, label: &str) -> anyhow::Error {
        if let Some(holder) = Self::describe_port_holder(port) {
            anyhow!("{label} port {port} is already in use by {holder}.")
        } else {
            anyhow!("{label} port {port} is already in use by another application.")
        }
    }

    pub fn cleanup_orphan_pids(paths: &PortablePaths) {
        for (pid_name, expected_exe) in [("httpd.pid", "httpd.exe"), ("nginx.pid", "nginx.exe")] {
            let pid_file = paths.tmp.join(pid_name);
            if let Ok(raw) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = raw.trim().parse::<u32>() {
                    if let Some(img) = image_name_for_pid(pid) {
                        if img.eq_ignore_ascii_case(expected_exe) {
                            kill_process_tree(pid);
                        }
                    }
                }
                let _ = fs::remove_file(pid_file);
            }
        }
    }

    pub fn get_statuses(&self, config: &AppConfig, inventory: &Inventory) -> Vec<ServiceStatus> {
        let mut procs = self.procs.lock().unwrap();
        let errors = self.errors.lock().unwrap();

        let mut list = Vec::with_capacity(ALL_SERVICES.len());
        for id in ALL_SERVICES {
            let active = get_active_binary(inventory, config, id);
            let installed = active.is_some();
            let (port, secondary_port, label) = match id {
                "nginx" => (config.ports.nginx_http, Some(config.ports.nginx_https), "Nginx"),
                "apache" => (
                    config.ports.apache_http,
                    Some(config.ports.apache_https),
                    "Apache",
                ),
                "php" => (config.ports.php_cgi, None, "PHP FastCGI"),
                "mysql" => (config.ports.mysql, None, "MariaDB / MySQL"),
                "mailpit" => (
                    config.ports.mailpit_http,
                    Some(config.ports.mailpit_smtp),
                    "Mailpit",
                ),
                _ => (0, None, id),
            };

            let mut state = "stopped".to_string();
            let mut pid = None;

            if let Some(entry) = procs.get_mut(id) {
                let alive = if let Some(child) = entry.child.as_mut() {
                    match child.try_wait() {
                        Ok(None) => true,
                        Ok(Some(_)) | Err(_) => false,
                    }
                } else {
                    // Supervised or detached (like php-cgi respawn loop or nginx master)
                    entry.desired_running && is_pid_alive(entry.pid)
                };

                if alive {
                    state = "running".to_string();
                    pid = Some(entry.pid);
                } else if entry.desired_running && id == "php" {
                    state = "starting".to_string();
                } else {
                    entry.desired_running = false;
                }
            }

            let last_error = errors.get(id).cloned();
            if state == "stopped" && last_error.is_some() {
                state = "error".to_string();
            }

            list.push(ServiceStatus {
                id: id.to_string(),
                label: label.to_string(),
                state,
                port,
                secondary_port,
                pid,
                version: active.map(|b| b.version.clone()),
                source: active.map(|b| b.source.clone()),
                installed,
                last_error,
            });
        }

        list
    }

    pub fn start_service(
        &self,
        id: &str,
        paths: &PortablePaths,
        config: &AppConfig,
        inventory: &Inventory,
        app: Option<&AppHandle>,
    ) -> Result<()> {
        if !ALL_SERVICES.contains(&id) {
            return Err(anyhow!("Unknown service: {id}"));
        }

        // Clear previous error
        self.errors.lock().unwrap().remove(id);

        // Already running?
        {
            let mut procs = self.procs.lock().unwrap();
            if let Some(existing) = procs.get_mut(id) {
                let alive = existing
                    .child
                    .as_mut()
                    .map(|c| matches!(c.try_wait(), Ok(None)))
                    .unwrap_or_else(|| existing.desired_running && is_pid_alive(existing.pid));
                if alive {
                    return Ok(());
                }
            }
        }

        let bin = get_active_binary(inventory, config, id)
            .ok_or_else(|| anyhow!("No binary installed or selected for '{id}'"))?;

        let result = match id {
            "nginx" => {
                self.sync_pinned_php_workers(paths, config, inventory, app)?;
                self.start_nginx(bin, paths, config, inventory)
            }
            "apache" => {
                self.sync_pinned_php_workers(paths, config, inventory, app)?;
                self.start_apache(bin, paths, config, inventory)
            }
            "php" => {
                self.start_php(bin, paths, config, app.cloned())?;
                self.sync_pinned_php_workers(paths, config, inventory, app)
            }
            "mysql" => self.start_mysql(bin, paths, config),
            "mailpit" => self.start_mailpit(bin, paths, config),
            _ => Err(anyhow!("Unsupported service {id}")),
        };

        match result {
            Ok(()) => {
                append_app_log(
                    &paths.logs,
                    app,
                    "INFO",
                    &format!("Started service '{id}' ({})", bin.version),
                );
                if let Some(h) = app {
                    let _ = h.emit("service-status-changed", id);
                }
                Ok(())
            }
            Err(err) => {
                let msg = format!("{err:#}");
                self.errors.lock().unwrap().insert(id.to_string(), msg.clone());
                append_app_log(
                    &paths.logs,
                    app,
                    "ERROR",
                    &format!("Failed to start '{id}': {msg}"),
                );
                if let Some(h) = app {
                    let _ = h.emit("service-status-changed", id);
                }
                Err(err)
            }
        }
    }

    pub fn stop_service(&self, id: &str, paths: &PortablePaths, app: Option<&AppHandle>) -> Result<()> {
        let entry = self.procs.lock().unwrap().remove(id);
        if let Some(mut proc_info) = entry {
            proc_info.desired_running = false;
            let mut exited_gracefully = false;

            if id == "mysql" {
                if let Some(exe) = proc_info.exe_path.as_ref() {
                    if let Some(bin_dir) = exe.parent() {
                        let admin = ["mariadb-admin.exe", "mysqladmin.exe"]
                            .into_iter()
                            .map(|name| bin_dir.join(name))
                            .find(|p| p.is_file());
                        if let Some(admin_exe) = admin {
                            let port_str = proc_info.port.to_string();
                            if let Ok(mut admin_child) = Command::new(&admin_exe)
                                .args(["-u", "root", "-P", &port_str, "--protocol=TCP", "shutdown"])
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .creation_flags(CREATE_NO_WINDOW)
                                .spawn()
                            {
                                let deadline = std::time::Instant::now()
                                    + std::time::Duration::from_millis(2000);
                                while std::time::Instant::now() < deadline {
                                    if let Some(child) = proc_info.child.as_mut() {
                                        if let Ok(Some(_)) = child.try_wait() {
                                            exited_gracefully = true;
                                            break;
                                        }
                                    }
                                    if let Ok(Some(status)) = admin_child.try_wait() {
                                        if !status.success() {
                                            break;
                                        }
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                                let _ = admin_child.kill();
                                let _ = admin_child.wait();
                            }
                        }
                    }
                }
            }

            if !exited_gracefully {
                if let Some(mut child) = proc_info.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                kill_process_tree(proc_info.pid);
            }
        }

        // Also clean up nginx or apache pid files if present
        if id == "nginx" {
            let pid_file = paths.tmp.join("nginx.pid");
            if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    kill_process_tree(pid);
                }
                let _ = fs::remove_file(pid_file);
            }
        } else if id == "apache" {
            let pid_file = paths.tmp.join("httpd.pid");
            if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    kill_process_tree(pid);
                }
                let _ = fs::remove_file(pid_file);
            }
        }

        if id == "php" {
            self.stop_all_pinned_php_workers();
        }

        self.errors.lock().unwrap().remove(id);
        append_app_log(&paths.logs, app, "INFO", &format!("Stopped service '{id}'"));
        if let Some(h) = app {
            let _ = h.emit("service-status-changed", id);
        }
        Ok(())
    }

    fn stop_all_pinned_php_workers(&self) {
        let pinned_keys: Vec<String> = {
            let procs = self.procs.lock().unwrap();
            procs
                .keys()
                .filter(|k| k.starts_with("php_pin:"))
                .cloned()
                .collect()
        };
        for key in pinned_keys {
            if let Some(mut proc_info) = self.procs.lock().unwrap().remove(&key) {
                proc_info.desired_running = false;
                if let Some(mut child) = proc_info.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                kill_process_tree(proc_info.pid);
            }
        }
    }

    pub fn stop_all(&self, paths: &PortablePaths, app: Option<&AppHandle>) {
        for id in ALL_SERVICES {
            let _ = self.stop_service(id, paths, app);
        }
        self.stop_all_pinned_php_workers();
    }

    fn start_nginx(
        &self,
        bin: &BinaryItem,
        paths: &PortablePaths,
        config: &AppConfig,
        inventory: &Inventory,
    ) -> Result<()> {
        if !Self::is_port_free(config.ports.nginx_http) {
            return Err(Self::port_in_use_error(config.ports.nginx_http, "Nginx HTTP"));
        }
        if config.auto_ssl && !Self::is_port_free(config.ports.nginx_https) {
            return Err(Self::port_in_use_error(config.ports.nginx_https, "Nginx HTTPS"));
        }

        let www_root = paths.active_www_root(config);
        let sites = vhosts::scan_sites(
            &www_root,
            config,
            inventory,
            &paths.etc_ssl,
            &paths.root,
            (config.ports.nginx_http, config.ports.nginx_https),
        );
        let apache_fcgi = is_apache_fcgi(inventory, config);
        vhosts::generate_vhost_configs(paths, config, &sites, apache_fcgi)?;

        let conf_path = write_nginx_main_conf(bin, paths, config)?;
        let nginx_dir = Path::new(&bin.root_dir);

        // Pre-validate nginx.conf syntax before spawning
        if let Ok(test_out) = Command::new(&bin.exe_path)
            .args([
                "-t",
                "-p",
                &slash_path(nginx_dir),
                "-c",
                &slash_path(&conf_path),
            ])
            .current_dir(nginx_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            if !test_out.status.success() {
                let err_msg = String::from_utf8_lossy(&test_out.stderr).trim().to_string();
                return Err(anyhow!("Nginx config syntax error: {err_msg}"));
            }
        }

        let log_err = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.logs.join("nginx-error.log"))?;

        let child = Command::new(&bin.exe_path)
            .args([
                "-p",
                &slash_path(nginx_dir),
                "-c",
                &slash_path(&conf_path),
            ])
            .current_dir(nginx_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_err))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .with_context(|| format!("Failed to spawn {}", bin.exe_path))?;

        let pid = child.id();
        self.procs.lock().unwrap().insert(
            "nginx".to_string(),
            RunningProc {
                child: Some(child),
                pid,
                desired_running: true,
                exe_path: Some(PathBuf::from(&bin.exe_path)),
                port: config.ports.nginx_http,
            },
        );
        Ok(())
    }

    fn start_apache(
        &self,
        bin: &BinaryItem,
        paths: &PortablePaths,
        config: &AppConfig,
        inventory: &Inventory,
    ) -> Result<()> {
        if !Self::is_port_free(config.ports.apache_http) {
            return Err(Self::port_in_use_error(config.ports.apache_http, "Apache HTTP"));
        }
        if config.auto_ssl && !Self::is_port_free(config.ports.apache_https) {
            return Err(Self::port_in_use_error(config.ports.apache_https, "Apache HTTPS"));
        }

        let www_root = paths.active_www_root(config);
        let sites = vhosts::scan_sites(
            &www_root,
            config,
            inventory,
            &paths.etc_ssl,
            &paths.root,
            (config.ports.apache_http, config.ports.apache_https),
        );
        let apache_fcgi = is_apache_fcgi(inventory, config);
        vhosts::generate_vhost_configs(paths, config, &sites, apache_fcgi)?;

        let conf_path = write_apache_main_conf(bin, paths, config, inventory)?;
        let apache_root = resolve_apache_srvroot(Path::new(&bin.root_dir));

        // Pre-validate httpd.conf syntax before spawning
        if let Ok(test_out) = Command::new(&bin.exe_path)
            .args(["-t", "-f", &slash_path(&conf_path)])
            .current_dir(&apache_root)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            if !test_out.status.success() {
                let err_msg = String::from_utf8_lossy(&test_out.stderr).trim().to_string();
                return Err(anyhow!("Apache config syntax error: {err_msg}"));
            }
        }

        let log_err = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.logs.join("apache-error.log"))?;

        let child = Command::new(&bin.exe_path)
            .args(["-f", &slash_path(&conf_path), "-DFOREGROUND"])
            .current_dir(&apache_root)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_err))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .with_context(|| format!("Failed to spawn {}", bin.exe_path))?;

        let pid = child.id();
        self.procs.lock().unwrap().insert(
            "apache".to_string(),
            RunningProc {
                child: Some(child),
                pid,
                desired_running: true,
                exe_path: Some(PathBuf::from(&bin.exe_path)),
                port: config.ports.apache_http,
            },
        );
        Ok(())
    }

    fn start_php(
        &self,
        bin: &BinaryItem,
        paths: &PortablePaths,
        config: &AppConfig,
        app: Option<AppHandle>,
    ) -> Result<()> {
        if !Self::is_port_free(config.ports.php_cgi) {
            return Err(Self::port_in_use_error(config.ports.php_cgi, "PHP FastCGI"));
        }

        let php_dir = Path::new(&bin.root_dir);
        let php_cgi = php_dir.join("php-cgi.exe");
        if !php_cgi.is_file() {
            return Err(anyhow!("php-cgi.exe not found in {}", php_dir.display()));
        }

        let ini_path = ensure_php_ini(php_dir, paths, config)?;
        let bind_addr = format!("127.0.0.1:{}", config.ports.php_cgi);
        let child = spawn_php_cgi(&php_cgi, php_dir, &bind_addr, &ini_path, &paths.logs)?;
        let pid = child.id();

        self.procs.lock().unwrap().insert(
            "php".to_string(),
            RunningProc {
                child: Some(child),
                pid,
                desired_running: true,
                exe_path: Some(php_cgi.clone()),
                port: config.ports.php_cgi,
            },
        );

        // Supervisor loop to auto-respawn php-cgi.exe on Windows exit
        let procs_ref = Arc::clone(&self.procs);
        let errors_ref = Arc::clone(&self.errors);
        let php_cgi_owned = php_cgi;
        let php_dir_owned = php_dir.to_path_buf();
        let ini_owned = ini_path;
        let logs_dir = paths.logs.clone();

        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                let mut should_respawn = false;
                {
                    let mut guard = procs_ref.lock().unwrap();
                    let Some(entry) = guard.get_mut("php") else {
                        break;
                    };
                    if !entry.desired_running {
                        break;
                    }
                    if let Some(child) = entry.child.as_mut() {
                        if let Ok(Some(_)) = child.try_wait() {
                            should_respawn = true;
                        }
                    }
                }

                if should_respawn {
                    match spawn_php_cgi(
                        &php_cgi_owned,
                        &php_dir_owned,
                        &bind_addr,
                        &ini_owned,
                        &logs_dir,
                    ) {
                        Ok(new_child) => {
                            let new_pid = new_child.id();
                            let mut guard = procs_ref.lock().unwrap();
                            if let Some(entry) = guard.get_mut("php") {
                                if entry.desired_running {
                                    entry.pid = new_pid;
                                    entry.child = Some(new_child);
                                } else {
                                    kill_process_tree(new_pid);
                                    break;
                                }
                            } else {
                                kill_process_tree(new_pid);
                                break;
                            }
                        }
                        Err(e) => {
                            errors_ref
                                .lock()
                                .unwrap()
                                .insert("php".to_string(), format!("Respawn failed: {e}"));
                            if let Some(h) = &app {
                                let _ = h.emit("service-status-changed", "php");
                            }
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    fn start_mysql(
        &self,
        bin: &BinaryItem,
        paths: &PortablePaths,
        config: &AppConfig,
    ) -> Result<()> {
        if !Self::is_port_free(config.ports.mysql) {
            return Err(Self::port_in_use_error(config.ports.mysql, "Database"));
        }

        let basedir = Path::new(&bin.root_dir);
        let my_ini = write_mysql_ini(basedir, paths, config)?;
        ensure_mysql_datadir_initialized(bin, basedir, paths)?;

        let log_err = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.logs.join("mysql-error.log"))?;

        let child = Command::new(&bin.exe_path)
            .args([
                &format!("--defaults-file={}", slash_path(&my_ini)),
                "--console",
            ])
            .current_dir(basedir)
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_err))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .with_context(|| format!("Failed to spawn {}", bin.exe_path))?;

        let pid = child.id();
        self.procs.lock().unwrap().insert(
            "mysql".to_string(),
            RunningProc {
                child: Some(child),
                pid,
                desired_running: true,
                exe_path: Some(PathBuf::from(&bin.exe_path)),
                port: config.ports.mysql,
            },
        );
        Ok(())
    }

    fn start_mailpit(
        &self,
        bin: &BinaryItem,
        paths: &PortablePaths,
        config: &AppConfig,
    ) -> Result<()> {
        if !Self::is_port_free(config.ports.mailpit_http) {
            return Err(Self::port_in_use_error(config.ports.mailpit_http, "Mailpit UI"));
        }
        if !Self::is_port_free(config.ports.mailpit_smtp) {
            return Err(Self::port_in_use_error(config.ports.mailpit_smtp, "Mailpit SMTP"));
        }

        let log_out = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.logs.join("mailpit.log"))?;
        let log_err = log_out.try_clone()?;

        let child = Command::new(&bin.exe_path)
            .args([
                "--smtp",
                &format!("127.0.0.1:{}", config.ports.mailpit_smtp),
                "--listen",
                &format!("127.0.0.1:{}", config.ports.mailpit_http),
            ])
            .current_dir(&bin.root_dir)
            .stdout(Stdio::from(log_out))
            .stderr(Stdio::from(log_err))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .with_context(|| format!("Failed to spawn {}", bin.exe_path))?;

        let pid = child.id();
        self.procs.lock().unwrap().insert(
            "mailpit".to_string(),
            RunningProc {
                child: Some(child),
                pid,
                desired_running: true,
                exe_path: Some(PathBuf::from(&bin.exe_path)),
                port: config.ports.mailpit_http,
            },
        );
        Ok(())
    }
}

fn spawn_php_cgi(
    php_cgi: &Path,
    php_dir: &Path,
    bind_addr: &str,
    ini_path: &Path,
    logs_dir: &Path,
) -> Result<Child> {
    let log_err = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("php-cgi.log"))?;

    Command::new(php_cgi)
        .args(["-b", bind_addr, "-c", &ini_path.to_string_lossy()])
        .env("PHP_FCGI_MAX_REQUESTS", "0")
        .env("PHP_FCGI_CHILDREN", "0")
        .current_dir(php_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_err))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .with_context(|| format!("Failed to spawn {}", php_cgi.display()))
}

pub fn pinned_php_ini_filename(bin: &BinaryItem) -> String {
    let raw = format!(
        "{}-{}",
        bin.version,
        bin.variant.as_deref().unwrap_or("nts")
    );
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("php-{sanitized}.ini")
}

pub fn ensure_php_ini(php_dir: &Path, paths: &PortablePaths, config: &AppConfig) -> Result<PathBuf> {
    ensure_php_ini_for_version(php_dir, paths, config, "php.ini")
}

pub fn ensure_php_ini_for_version(
    php_dir: &Path,
    paths: &PortablePaths,
    config: &AppConfig,
    file_name: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(&paths.etc_php)?;
    let ini_path = paths.etc_php.join(file_name);
    let ext_dir = slash_path(&php_dir.join("ext"));
    let target_ext_line = format!("extension_dir = \"{ext_dir}\"");
    let target_smtp_line = format!("smtp_port = {}", config.ports.mailpit_smtp);

    if !ini_path.is_file() {
        let dev_ini = php_dir.join("php.ini-development");
        let mut content = if dev_ini.is_file() {
            fs::read_to_string(&dev_ini).unwrap_or_default()
        } else {
            "[PHP]\nmax_execution_time = 120\nmemory_limit = 512M\nupload_max_filesize = 128M\npost_max_size = 128M\n".to_string()
        };

        content.push_str(&format!(
            "\n; --- EnvStackUI Overrides ---\n{target_ext_line}\nSMTP = 127.0.0.1\n{target_smtp_line}\ncgi.fix_pathinfo = 1\n"
        ));

        for ext in [
            "curl", "fileinfo", "gd", "intl", "mbstring", "mysqli", "openssl", "pdo_mysql",
            "pdo_sqlite", "zip",
        ] {
            content.push_str(&format!("extension={ext}\n"));
        }

        fs::write(&ini_path, content)?;
    } else {
        // Keep active PHP extension_dir and Mailpit SMTP port synced across version switches
        let raw = fs::read_to_string(&ini_path).unwrap_or_default();
        let mut updated_lines = Vec::new();
        let mut found_ext = false;
        let mut found_smtp = false;
        let mut modified = false;

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("extension_dir") && trimmed.contains('=') && !trimmed.starts_with(';') {
                found_ext = true;
                if trimmed != target_ext_line {
                    updated_lines.push(target_ext_line.clone());
                    modified = true;
                    continue;
                }
            } else if trimmed.starts_with("smtp_port") && trimmed.contains('=') && !trimmed.starts_with(';') {
                found_smtp = true;
                if trimmed != target_smtp_line {
                    updated_lines.push(target_smtp_line.clone());
                    modified = true;
                    continue;
                }
            }
            updated_lines.push(line.to_string());
        }

        if !found_ext {
            updated_lines.push(target_ext_line);
            modified = true;
        }
        if !found_smtp {
            updated_lines.push(target_smtp_line);
            modified = true;
        }

        if modified {
            let mut next = updated_lines.join("\n");
            if !next.ends_with('\n') {
                next.push('\n');
            }
            fs::write(&ini_path, next)?;
        }
    }

    Ok(ini_path)
}

fn resolve_apache_srvroot(dir: &Path) -> PathBuf {
    if dir.join("Apache24").join("bin").join("httpd.exe").is_file() {
        dir.join("Apache24")
    } else {
        dir.to_path_buf()
    }
}

fn is_apache_fcgi(inventory: &Inventory, config: &AppConfig) -> bool {
    if let Some(php) = get_active_binary(inventory, config, "php") {
        php.variant.as_deref() != Some("TS")
    } else {
        true
    }
}

fn write_nginx_main_conf(
    bin: &BinaryItem,
    paths: &PortablePaths,
    config: &AppConfig,
) -> Result<PathBuf> {
    let nginx_dir = Path::new(&bin.root_dir);
    let mime_types = if nginx_dir.join("conf").join("mime.types").is_file() {
        slash_path(&nginx_dir.join("conf").join("mime.types"))
    } else {
        slash_path(&nginx_dir.join("mime.types"))
    };

    // Ensure fastcgi_params exists in etc/nginx
    let fastcgi_params_dst = paths.etc_nginx.join("fastcgi_params");
    if !fastcgi_params_dst.is_file() {
        let src = nginx_dir.join("conf").join("fastcgi_params");
        if src.is_file() {
            let _ = fs::copy(src, &fastcgi_params_dst);
        } else {
            fs::write(
                &fastcgi_params_dst,
                "fastcgi_param QUERY_STRING $query_string;\nfastcgi_param REQUEST_METHOD $request_method;\nfastcgi_param CONTENT_TYPE $content_type;\nfastcgi_param CONTENT_LENGTH $content_length;\nfastcgi_param SCRIPT_NAME $fastcgi_script_name;\nfastcgi_param REQUEST_URI $request_uri;\nfastcgi_param DOCUMENT_URI $document_uri;\nfastcgi_param DOCUMENT_ROOT $document_root;\nfastcgi_param SERVER_PROTOCOL $server_protocol;\nfastcgi_param REQUEST_SCHEME $scheme;\nfastcgi_param HTTPS $https if_not_empty;\nfastcgi_param GATEWAY_INTERFACE CGI/1.1;\nfastcgi_param SERVER_SOFTWARE nginx/$nginx_version;\nfastcgi_param REMOTE_ADDR $remote_addr;\nfastcgi_param REMOTE_PORT $remote_port;\nfastcgi_param SERVER_ADDR $server_addr;\nfastcgi_param SERVER_PORT $server_port;\nfastcgi_param SERVER_NAME $server_name;\nfastcgi_param REDIRECT_STATUS 200;\n",
            )?;
        }
    }

    let www_root = slash_path(&paths.active_www_root(config));
    let adminer_dir = slash_path(&paths.usr_adminer);
    let sites_glob = slash_path(&paths.etc_nginx_sites.join("*.conf"));
    let access_log = slash_path(&paths.logs.join("nginx-access.log"));
    let error_log = slash_path(&paths.logs.join("nginx-error.log"));
    let pid_file = slash_path(&paths.tmp.join("nginx.pid"));
    let temp_root = slash_path(&paths.tmp);

    let conf = format!(
        r#"worker_processes  auto;
error_log  "{error_log}" warn;
pid        "{pid_file}";

events {{
    worker_connections  1024;
}}

http {{
    include       "{mime_types}";
    default_type  application/octet-stream;

    sendfile        on;
    keepalive_timeout  65;
    client_max_body_size 128M;
    server_names_hash_bucket_size 64;

    access_log  "{access_log}";
    client_body_temp_path "{temp_root}/nginx_body";
    proxy_temp_path "{temp_root}/nginx_proxy";
    fastcgi_temp_path "{temp_root}/nginx_fastcgi";

    server {{
        listen {http_port} default_server;
        server_name localhost 127.0.0.1;
        root "{www_root}";
        index index.php index.html index.htm;
        autoindex on;

        location ^~ /__adminer {{
            alias "{adminer_dir}";
            index index.php;
            location ~ \.php$ {{
                fastcgi_pass 127.0.0.1:{php_port};
                fastcgi_index index.php;
                fastcgi_param SCRIPT_FILENAME $request_filename;
                include "{fcgi_params}";
            }}
        }}

        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}

        location ~ \.php$ {{
            try_files $uri =404;
            fastcgi_pass 127.0.0.1:{php_port};
            fastcgi_index index.php;
            fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
            include "{fcgi_params}";
        }}
    }}

    include "{sites_glob}";
}}
"#,
        http_port = config.ports.nginx_http,
        php_port = config.ports.php_cgi,
        fcgi_params = slash_path(&fastcgi_params_dst)
    );

    let conf_path = paths.etc_nginx.join("nginx.conf");
    fs::write(&conf_path, conf)?;
    Ok(conf_path)
}

fn write_apache_main_conf(
    bin: &BinaryItem,
    paths: &PortablePaths,
    config: &AppConfig,
    inventory: &Inventory,
) -> Result<PathBuf> {
    let srvroot = resolve_apache_srvroot(Path::new(&bin.root_dir));
    let srvroot_slash = slash_path(&srvroot);
    let www_root = slash_path(&paths.active_www_root(config));
    let adminer_dir = slash_path(&paths.usr_adminer);
    let sites_glob = slash_path(&paths.etc_apache_sites.join("*.conf"));
    let error_log = slash_path(&paths.logs.join("apache-error.log"));
    let access_log = slash_path(&paths.logs.join("apache-access.log"));
    let pid_file = slash_path(&paths.tmp.join("httpd.pid"));
    let ssl_scache = slash_path(&paths.tmp.join("ssl_scache"));

    let active_php = get_active_binary(inventory, config, "php");
    let mut php_module_block = String::new();
    let mut fcgi_handler_block = String::new();

    if let Some(php) = active_php {
        let php_dir = Path::new(&php.root_dir);
        let dll = php_dir.join("php8apache2_4.dll");
        if php.variant.as_deref() == Some("TS") && dll.is_file() {
            let _ = ensure_php_ini(php_dir, paths, config);
            let ini_dir = slash_path(&paths.etc_php);
            for dep_name in [
                "libcrypto-3-x64.dll",
                "libssl-3-x64.dll",
                "libsodium.dll",
                "libssh2.dll",
                "libsqlite3.dll",
                "nghttp2.dll",
                "libpq.dll",
            ] {
                let dep_path = php_dir.join(dep_name);
                if dep_path.is_file() {
                    php_module_block.push_str(&format!(
                        "LoadFile \"{}\"\n",
                        slash_path(&dep_path)
                    ));
                }
            }
            php_module_block.push_str(&format!(
                "LoadModule php_module \"{}\"\nPHPIniDir \"{}\"\n<IfModule mime_module>\n    AddType application/x-httpd-php .php\n</IfModule>\n",
                slash_path(&dll),
                ini_dir
            ));
            if !vhosts::resolve_pinned_php_ports(config, inventory).is_empty() {
                php_module_block.push_str(
                    "LoadModule proxy_module modules/mod_proxy.so\nLoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so\nProxyTimeout 120\n",
                );
            }
        } else {
            php_module_block = "LoadModule proxy_module modules/mod_proxy.so\nLoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so\n".to_string();
            fcgi_handler_block = format!(
                r#"ProxyTimeout 120
ProxyFCGIBackendType GENERIC
ProxyFCGISetEnvIf "reqenv('SCRIPT_FILENAME') =~ m#^(?:proxy:fcgi://[^/]+)?/([A-Za-z]:/.*)#" SCRIPT_FILENAME "$1"
<FilesMatch \.php$>
    SetHandler "proxy:fcgi://127.0.0.1:{}/"
</FilesMatch>
"#,
                config.ports.php_cgi
            );
        }
    }

    let conf = format!(
        r#"Define SRVROOT "{srvroot_slash}"
ServerRoot "${{SRVROOT}}"
PidFile "{pid_file}"

Listen {http_port}
Listen {https_port}

LoadModule access_compat_module modules/mod_access_compat.so
LoadModule alias_module modules/mod_alias.so
LoadModule authz_core_module modules/mod_authz_core.so
LoadModule authz_host_module modules/mod_authz_host.so
LoadModule autoindex_module modules/mod_autoindex.so
LoadModule dir_module modules/mod_dir.so
LoadModule env_module modules/mod_env.so
LoadModule headers_module modules/mod_headers.so
LoadModule log_config_module modules/mod_log_config.so
LoadModule mime_module modules/mod_mime.so
LoadModule rewrite_module modules/mod_rewrite.so
LoadModule setenvif_module modules/mod_setenvif.so
LoadModule socache_shmcb_module modules/mod_socache_shmcb.so
LoadModule ssl_module modules/mod_ssl.so
{php_module_block}

ServerName localhost:{http_port}
ErrorLog "{error_log}"
LogLevel warn
LogFormat "%h %l %u %t \"%r\" %>s %b \"%{{Referer}}i\" \"%{{User-Agent}}i\"" combined
LogFormat "%h %l %u %t \"%r\" %>s %b" common
CustomLog "{access_log}" combined

AcceptFilter http none
AcceptFilter https none
EnableSendfile off
EnableMMAP off
SSLSessionCache "shmcb:{ssl_scache}(512000)"
SSLSessionCacheTimeout 300

TypesConfig conf/mime.types
DirectoryIndex index.php index.html index.htm

<Directory />
    AllowOverride none
    Require all denied
</Directory>

DocumentRoot "{www_root}"
<Directory "{www_root}">
    Options Indexes FollowSymLinks
    AllowOverride All
    Require all granted
</Directory>

Alias /__adminer "{adminer_dir}"
<Directory "{adminer_dir}">
    Options Indexes FollowSymLinks
    AllowOverride All
    Require all granted
</Directory>

{fcgi_handler_block}

IncludeOptional "{sites_glob}"
"#,
        http_port = config.ports.apache_http,
        https_port = config.ports.apache_https,
    );

    let conf_path = paths.etc_apache.join("httpd.conf");
    fs::write(&conf_path, conf)?;
    Ok(conf_path)
}

fn write_mysql_ini(basedir: &Path, paths: &PortablePaths, config: &AppConfig) -> Result<PathBuf> {
    fs::create_dir_all(&paths.etc_mysql)?;
    fs::create_dir_all(&paths.data_mysql)?;
    let ini_path = paths.etc_mysql.join("my.ini");

    let ini = format!(
        r#"[mysqld]
port={}
basedir="{}"
datadir="{}"
log-error="{}"
character-set-server=utf8mb4
collation-server=utf8mb4_unicode_ci
skip-external-locking

[client]
port={}
default-character-set=utf8mb4
"#,
        config.ports.mysql,
        slash_path(basedir),
        slash_path(&paths.data_mysql),
        slash_path(&paths.logs.join("mysql-error.log")),
        config.ports.mysql,
    );

    fs::write(&ini_path, ini)?;
    Ok(ini_path)
}

fn ensure_mysql_datadir_initialized(
    bin: &BinaryItem,
    basedir: &Path,
    paths: &PortablePaths,
) -> Result<()> {
    let mysql_sys_dir = paths.data_mysql.join("mysql");
    if mysql_sys_dir.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(&paths.data_mysql)?;
    let mariadb_init = basedir.join("bin").join("mariadb-install-db.exe");
    let mysql_init = basedir.join("bin").join("mysql_install_db.exe");

    let output = if mariadb_init.is_file() {
        Command::new(&mariadb_init)
            .args([&format!("--datadir={}", slash_path(&paths.data_mysql))])
            .current_dir(basedir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("Failed to initialize MariaDB data directory")?
    } else if mysql_init.is_file() {
        Command::new(&mysql_init)
            .args([&format!("--datadir={}", slash_path(&paths.data_mysql))])
            .current_dir(basedir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("Failed to initialize MariaDB/MySQL data directory")?
    } else {
        Command::new(&bin.exe_path)
            .args([
                "--initialize-insecure",
                &format!("--basedir={}", slash_path(basedir)),
                &format!("--datadir={}", slash_path(&paths.data_mysql)),
            ])
            .current_dir(basedir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("Failed to run mysqld --initialize-insecure")?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "Database initialization failed: {stderr} {stdout}"
        ));
    }

    Ok(())
}

fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

fn image_name_for_pid(pid: u32) -> Option<String> {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    if !line.starts_with('"') {
        return None;
    }
    let inner = line.trim_start_matches('"');
    let name = inner.split('"').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains(&pid.to_string())
        }
        Err(_) => false,
    }
}

impl ServiceManager {
    pub fn sync_pinned_php_workers(
        &self,
        paths: &PortablePaths,
        config: &AppConfig,
        inventory: &Inventory,
        app: Option<&AppHandle>,
    ) -> Result<()> {
        let pinned_ports = vhosts::resolve_pinned_php_ports(config, inventory);
        let php_bins = inventory.binaries.get("php").map(Vec::as_slice).unwrap_or(&[]);

        let mut desired_by_port: HashMap<u16, BinaryItem> = HashMap::new();
        for (bin_id, port) in pinned_ports {
            if let Some(bin) = php_bins.iter().find(|b| b.id == bin_id) {
                desired_by_port.insert(port, bin.clone());
            }
        }

        // Stop workers whose port is no longer pinned or whose binary changed
        let existing_keys: Vec<(String, u16, Option<PathBuf>)> = {
            let procs = self.procs.lock().unwrap();
            procs
                .iter()
                .filter(|(k, _)| k.starts_with("php_pin:"))
                .map(|(k, v)| (k.clone(), v.port, v.exe_path.clone()))
                .collect()
        };

        for (key, port, exe_opt) in existing_keys {
            let keep = desired_by_port
                .get(&port)
                .map(|bin| {
                    let expected_cgi = Path::new(&bin.root_dir).join("php-cgi.exe");
                    exe_opt.as_ref() == Some(&expected_cgi)
                })
                .unwrap_or(false);

            if !keep {
                if let Some(mut proc_info) = self.procs.lock().unwrap().remove(&key) {
                    proc_info.desired_running = false;
                    if let Some(mut child) = proc_info.child.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    kill_process_tree(proc_info.pid);
                }
            }
        }

        // Start missing pinned php-cgi workers
        for (port, bin) in desired_by_port {
            let key = format!("php_pin:{port}");
            let already_alive = {
                let mut procs = self.procs.lock().unwrap();
                procs
                    .get_mut(&key)
                    .map(|entry| {
                        entry
                            .child
                            .as_mut()
                            .map(|c| matches!(c.try_wait(), Ok(None)))
                            .unwrap_or_else(|| entry.desired_running && is_pid_alive(entry.pid))
                    })
                    .unwrap_or(false)
            };

            if already_alive {
                continue;
            }

            if !Self::is_port_free(port) {
                return Err(Self::port_in_use_error(
                    port,
                    &format!("Pinned PHP ({})", bin.version),
                ));
            }

            let php_dir = Path::new(&bin.root_dir);
            let php_cgi = php_dir.join("php-cgi.exe");
            if !php_cgi.is_file() {
                return Err(anyhow!(
                    "php-cgi.exe not found for pinned PHP {} in {}",
                    bin.version,
                    php_dir.display()
                ));
            }

            let ini_name = pinned_php_ini_filename(&bin);
            let ini_path = ensure_php_ini_for_version(php_dir, paths, config, &ini_name)?;
            let bind_addr = format!("127.0.0.1:{port}");
            let child = spawn_php_cgi(&php_cgi, php_dir, &bind_addr, &ini_path, &paths.logs)?;
            let pid = child.id();

            self.procs.lock().unwrap().insert(
                key.clone(),
                RunningProc {
                    child: Some(child),
                    pid,
                    desired_running: true,
                    exe_path: Some(php_cgi.clone()),
                    port,
                },
            );

            append_app_log(
                &paths.logs,
                app,
                "INFO",
                &format!("Started pinned PHP {} FastCGI worker on 127.0.0.1:{port}", bin.version),
            );

            let procs_ref = Arc::clone(&self.procs);
            let php_cgi_owned = php_cgi;
            let php_dir_owned = php_dir.to_path_buf();
            let ini_owned = ini_path;
            let logs_dir = paths.logs.clone();
            let key_owned = key;

            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    let mut should_respawn = false;
                    {
                        let mut guard = procs_ref.lock().unwrap();
                        let Some(entry) = guard.get_mut(&key_owned) else {
                            break;
                        };
                        if !entry.desired_running {
                            break;
                        }
                        if let Some(child) = entry.child.as_mut() {
                            if let Ok(Some(_)) = child.try_wait() {
                                should_respawn = true;
                            }
                        }
                    }

                    if should_respawn {
                        match spawn_php_cgi(
                            &php_cgi_owned,
                            &php_dir_owned,
                            &bind_addr,
                            &ini_owned,
                            &logs_dir,
                        ) {
                            Ok(new_child) => {
                                let new_pid = new_child.id();
                                let mut guard = procs_ref.lock().unwrap();
                                if let Some(entry) = guard.get_mut(&key_owned) {
                                    if entry.desired_running {
                                        entry.pid = new_pid;
                                        entry.child = Some(new_child);
                                    } else {
                                        kill_process_tree(new_pid);
                                        break;
                                    }
                                } else {
                                    kill_process_tree(new_pid);
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            });
        }

        Ok(())
    }
}

pub fn validate_db_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 63 {
        return Err(anyhow!("Database name must be 1 to 63 characters."));
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(anyhow!(
            "Database name must start with an ASCII letter or underscore."
        ));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(anyhow!(
            "Database name may only contain letters, numbers, and underscores."
        ));
    }
    Ok(trimmed.to_string())
}

fn find_mysql_client_exe(config: &AppConfig, inventory: &Inventory) -> Result<PathBuf> {
    let bin = get_active_binary(inventory, config, "mysql")
        .ok_or_else(|| anyhow!("No MariaDB / MySQL binary is configured."))?;
    let exe_parent = Path::new(&bin.exe_path)
        .parent()
        .unwrap_or_else(|| Path::new(&bin.bin_dir));
    for name in ["mariadb.exe", "mysql.exe"] {
        let candidate = exe_parent.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "mariadb.exe or mysql.exe was not found next to {}",
        bin.exe_path
    ))
}

pub fn list_mysql_databases(config: &AppConfig, inventory: &Inventory) -> Result<Vec<String>> {
    let client = find_mysql_client_exe(config, inventory)?;
    let port_str = config.ports.mysql.to_string();
    let out = Command::new(&client)
        .args([
            "-u",
            "root",
            "-P",
            &port_str,
            "--protocol=TCP",
            "-N",
            "-B",
            "-e",
            "SHOW DATABASES;",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .with_context(|| format!("Failed to execute {}", client.display()))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow!(
            "Could not connect to MariaDB / MySQL on port {port_str}: {stderr}"
        ));
    }

    let mut list: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| {
            !s.is_empty()
                && !matches!(
                    s.to_ascii_lowercase().as_str(),
                    "information_schema" | "mysql" | "performance_schema" | "sys"
                )
        })
        .map(String::from)
        .collect();
    list.sort_by_key(|a| a.to_lowercase());
    Ok(list)
}

pub fn create_mysql_database(
    name: &str,
    config: &AppConfig,
    inventory: &Inventory,
) -> Result<Vec<String>> {
    let safe_name = validate_db_name(name)?;
    let client = find_mysql_client_exe(config, inventory)?;
    let port_str = config.ports.mysql.to_string();
    let sql = format!(
        "CREATE DATABASE IF NOT EXISTS `{safe_name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
    );

    let out = Command::new(&client)
        .args(["-u", "root", "-P", &port_str, "--protocol=TCP", "-e", &sql])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .with_context(|| format!("Failed to execute {}", client.display()))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow!("Failed to create database '{safe_name}': {stderr}"));
    }

    list_mysql_databases(config, inventory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_db_name_accepts_valid_and_rejects_unsafe() {
        assert_eq!(validate_db_name("  my_app_db ").unwrap(), "my_app_db");
        assert_eq!(validate_db_name("_internal2").unwrap(), "_internal2");

        assert!(validate_db_name("").is_err());
        assert!(validate_db_name("1starts_with_digit").is_err());
        assert!(validate_db_name("bad-hyphen").is_err());
        assert!(validate_db_name("bad; DROP TABLE users").is_err());
        assert!(validate_db_name("quote`db").is_err());
    }

    #[test]
    fn ensure_php_ini_for_version_writes_isolated_ini() {
        let base = std::env::temp_dir().join(format!("envstack_ini_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        let php_dir = base.join("bin").join("php").join("php-8.2.28");
        fs::create_dir_all(&php_dir).unwrap();

        let cfg = AppConfig::default();
        let ini_path =
            ensure_php_ini_for_version(&php_dir, &paths, &cfg, "php-8.2.28-nts.ini").unwrap();
        assert!(ini_path.is_file());
        assert_eq!(
            ini_path.file_name().and_then(|s| s.to_str()),
            Some("php-8.2.28-nts.ini")
        );
        let content = fs::read_to_string(&ini_path).unwrap();
        assert!(content.contains("extension_dir"));
        assert!(content.contains("php-8.2.28/ext"));

        let _ = fs::remove_dir_all(&base);
    }
}
