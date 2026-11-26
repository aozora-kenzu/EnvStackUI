use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFileInfo {
    pub name: String,
    pub size_bytes: u64,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTailPayload {
    pub name: String,
    pub content: String,
    pub total_lines: usize,
}

const KNOWN_LOGS: [&str; 7] = [
    "envstack.log",
    "nginx-error.log",
    "nginx-access.log",
    "apache-error.log",
    "php-cgi.log",
    "mysql-error.log",
    "mailpit.log",
];

pub fn append_app_log(logs_dir: &Path, app: Option<&AppHandle>, level: &str, message: &str) {
    let _ = fs::create_dir_all(logs_dir);
    let path = logs_dir.join("envstack.log");
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("[{ts}] [{level}] {message}\n");

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }

    if let Some(handle) = app {
        let _ = handle.emit("log-appended", line);
    }
}

pub fn list_logs(logs_dir: &Path) -> Vec<LogFileInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();

    for name in KNOWN_LOGS {
        let path = logs_dir.join(name);
        let meta = fs::metadata(&path).ok();
        items.push(LogFileInfo {
            name: name.to_string(),
            size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            exists: meta.is_some(),
        });
        seen.insert(name.to_string());
    }

    if let Ok(entries) = fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if name.ends_with(".log") && seen.insert(name.to_string()) {
                        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        items.push(LogFileInfo {
                            name: name.to_string(),
                            size_bytes,
                            exists: true,
                        });
                    }
                }
            }
        }
    }

    items
}

pub fn read_log_tail(logs_dir: &Path, name: &str, max_lines: usize) -> LogTailPayload {
    let safe_name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("envstack.log");
    let path = logs_dir.join(safe_name);
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = raw.lines().collect();
    let total_lines = lines.len();
    let start = total_lines.saturating_sub(max_lines.max(1));
    let content = lines[start..].join("\n");

    LogTailPayload {
        name: safe_name.to_string(),
        content,
        total_lines,
    }
}

pub fn clear_log(logs_dir: &Path, name: &str) -> std::io::Result<()> {
    let safe_name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("envstack.log");
    let path = logs_dir.join(safe_name);
    if path.exists() {
        fs::write(path, "")?;
    }
    Ok(())
}
