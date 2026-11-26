use crate::portable::{AppConfig, CREATE_NO_WINDOW};
use crate::scanner::{get_active_binary, Inventory};
use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

pub fn build_active_bin_dirs(config: &AppConfig, inventory: &Inventory) -> Vec<String> {
    let order = [
        "php", "node", "composer", "git", "python", "go", "mysql", "nginx", "apache", "mailpit",
    ];
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();

    for kind in order {
        if let Some(bin) = get_active_binary(inventory, config, kind) {
            for candidate in [&bin.bin_dir, &bin.root_dir] {
                let p = Path::new(candidate);
                if p.is_dir() && seen.insert(candidate.to_lowercase()) {
                    dirs.push(candidate.clone());
                }
            }
            // Also include Scripts/ for portable Python pip
            if kind == "python" {
                let scripts = Path::new(&bin.root_dir).join("Scripts");
                if scripts.is_dir() {
                    let s = scripts.to_string_lossy().to_string();
                    if seen.insert(s.to_lowercase()) {
                        dirs.push(s);
                    }
                }
            }
        }
    }

    dirs
}

pub fn build_combined_path(bin_dirs: &[String]) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    if bin_dirs.is_empty() {
        return current;
    }
    format!("{};{}", bin_dirs.join(";"), current)
}

pub fn open_terminal(
    profile: &str,
    cwd: &Path,
    config: &AppConfig,
    inventory: &Inventory,
) -> Result<()> {
    let bin_dirs = build_active_bin_dirs(config, inventory);
    let combined_path = build_combined_path(&bin_dirs);
    let work_dir = if cwd.is_dir() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| Path::new("C:\\").to_path_buf())
    };

    let banner = "EnvStackUI Portable Shell";

    match profile {
        "wt" => {
            if Command::new("wt.exe")
                .args([
                    "-d",
                    &work_dir.to_string_lossy(),
                    "powershell.exe",
                    "-NoExit",
                    "-Command",
                    &format!("Write-Host '{banner}' -ForegroundColor Cyan"),
                ])
                .env("PATH", &combined_path)
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
            // Fallback to powershell if Windows Terminal is not installed
            open_terminal("powershell", &work_dir, config, inventory)
        }
        "gitbash" => {
            let git_bash = get_active_binary(inventory, config, "git")
                .map(|g| Path::new(&g.root_dir).join("git-bash.exe"))
                .filter(|p| p.is_file())
                .or_else(|| {
                    let p = Path::new(r"C:\Program Files\Git\git-bash.exe");
                    p.is_file().then(|| p.to_path_buf())
                });

            if let Some(bash_exe) = git_bash {
                Command::new(bash_exe)
                    .arg(format!("--cd={}", work_dir.to_string_lossy()))
                    .env("PATH", &combined_path)
                    .spawn()
                    .context("Failed to launch Git Bash")?;
                return Ok(());
            }
            Err(anyhow!(
                "Git Bash (git-bash.exe) not found. Configure Git runtime or pick PowerShell."
            ))
        }
        "cmd" => {
            Command::new("cmd.exe")
                .args(["/K", &format!("title {banner}")])
                .current_dir(&work_dir)
                .env("PATH", &combined_path)
                .spawn()
                .context("Failed to launch Command Prompt")?;
            Ok(())
        }
        _ => {
            // Default: PowerShell
            let ps_exe = if has_command("pwsh.exe") {
                "pwsh.exe"
            } else {
                "powershell.exe"
            };
            Command::new(ps_exe)
                .args([
                    "-NoExit",
                    "-Command",
                    &format!("Write-Host '{banner}' -ForegroundColor Cyan"),
                ])
                .current_dir(&work_dir)
                .env("PATH", &combined_path)
                .spawn()
                .context("Failed to launch PowerShell")?;
            Ok(())
        }
    }
}

fn has_command(exe: &str) -> bool {
    Command::new("where")
        .arg(exe)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn sync_user_path(config: &mut AppConfig, inventory: &Inventory) -> Result<Vec<String>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu
        .create_subkey("Environment")
        .context("Failed to open HKCU\\Environment")?;

    let raw: String = env_key.get_value("Path").unwrap_or_default();
    let previous_managed: HashSet<String> = config
        .user_path_entries
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut entries: Vec<String> = raw
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !previous_managed.contains(&s.to_lowercase()))
        .map(String::from)
        .collect();

    let new_dirs = build_active_bin_dirs(config, inventory);
    let existing_lower: HashSet<String> = entries.iter().map(|s| s.to_lowercase()).collect();

    let mut added = Vec::new();
    for dir in &new_dirs {
        if !existing_lower.contains(&dir.to_lowercase()) {
            entries.push(dir.clone());
            added.push(dir.clone());
        }
    }

    let joined = entries.join(";");
    env_key
        .set_value("Path", &joined)
        .context("Failed to write HKCU\\Environment\\Path")?;

    config.user_path_entries = added.clone();
    Ok(added)
}

pub fn clear_user_path(config: &mut AppConfig) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu
        .create_subkey("Environment")
        .context("Failed to open HKCU\\Environment")?;

    let raw: String = env_key.get_value("Path").unwrap_or_default();
    let previous_managed: HashSet<String> = config
        .user_path_entries
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let remaining: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !previous_managed.contains(&s.to_lowercase()))
        .collect();

    env_key
        .set_value("Path", &remaining.join(";"))
        .context("Failed to clean HKCU\\Environment\\Path")?;

    config.user_path_entries.clear();
    Ok(())
}
