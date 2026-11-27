use crate::portable::{AppConfig, PortablePaths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryItem {
    /// Unique identifier for selection (canonical full directory path)
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    /// "portable" or "external"
    pub source: String,
    pub root_dir: String,
    pub exe_path: String,
    pub bin_dir: String,
    /// Optional metadata, e.g. "TS" / "NTS" for PHP or "MariaDB" / "MySQL"
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDbClient {
    pub name: String,
    pub exe_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub binaries: BTreeMap<String, Vec<BinaryItem>>,
    pub active_versions: HashMap<String, String>,
    pub detected_db_clients: Vec<ExternalDbClient>,
}

fn kind_aliases(kind: &str) -> &'static [&'static str] {
    match kind {
        "nginx" => &["nginx"],
        "apache" => &["apache", "httpd"],
        "php" => &["php"],
        "mysql" => &["mysql", "mariadb"],
        "postgres" => &["postgres", "postgresql", "pgsql"],
        "redis" => &["redis", "valkey"],
        "mailpit" => &["mailpit"],
        "node" => &["node", "nodejs"],
        "composer" => &["composer"],
        "git" => &["git"],
        "python" => &["python"],
        "go" => &["go", "golang"],
        _ => &[],
    }
}

fn candidate_exe_rel_paths(kind: &str) -> &'static [&'static str] {
    match kind {
        "nginx" => &["nginx.exe"],
        "apache" => &["bin/httpd.exe", "Apache24/bin/httpd.exe", "httpd.exe"],
        "php" => &["php.exe", "php-cgi.exe"],
        "mysql" => &[
            "bin/mysqld.exe",
            "bin/mariadbd.exe",
            "mysqld.exe",
            "mariadbd.exe",
        ],
        "postgres" => &["bin/postgres.exe", "pgsql/bin/postgres.exe", "postgres.exe"],
        "redis" => &["redis-server.exe", "bin/redis-server.exe", "valkey-server.exe"],
        "mailpit" => &["mailpit.exe"],
        "node" => &["node.exe"],
        "composer" => &["composer.bat", "composer.phar"],
        "git" => &["cmd/git.exe", "bin/git.exe", "git.exe"],
        "python" => &["python.exe"],
        "go" => &["bin/go.exe", "go.exe"],
        _ => &[],
    }
}

fn find_exe_in_dir(dir: &Path, kind: &str) -> Option<(PathBuf, PathBuf)> {
    for rel in candidate_exe_rel_paths(kind) {
        let candidate = dir.join(rel);
        if candidate.is_file() {
            let bin_dir = candidate.parent().unwrap_or(dir).to_path_buf();
            return Some((candidate, bin_dir));
        }
    }
    None
}

fn detect_variant(kind: &str, dir: &Path, exe: &Path) -> Option<String> {
    match kind {
        "php" => {
            if dir.join("php8apache2_4.dll").is_file() || dir.join("php7apache2_4.dll").is_file() {
                Some("TS".to_string())
            } else {
                Some("NTS".to_string())
            }
        }
        "mysql" => {
            let fname = exe
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if fname.contains("mariadb") {
                Some("MariaDB".to_string())
            } else {
                Some("MySQL".to_string())
            }
        }
        "postgres" => Some("PostgreSQL".to_string()),
        "redis" => {
            let fname = exe
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if fname.contains("valkey") {
                Some("Valkey".to_string())
            } else {
                Some("Redis".to_string())
            }
        }
        _ => None,
    }
}

fn clean_version_label(dir_name: &str, kind: &str) -> String {
    let lower = dir_name.to_lowercase();
    for prefix in [
        format!("{kind}-"),
        "httpd-".to_string(),
        "node-".to_string(),
        "mariadb-".to_string(),
        "mysql-".to_string(),
        "postgresql-".to_string(),
        "postgres-".to_string(),
        "redis-".to_string(),
        "valkey-".to_string(),
        "php-".to_string(),
    ] {
        if lower.starts_with(&prefix) && dir_name.len() > prefix.len() {
            return dir_name[prefix.len()..].to_string();
        }
    }
    dir_name.to_string()
}

fn scan_kind_root(
    kind: &str,
    cat_dir: &Path,
    source: &str,
    out: &mut Vec<BinaryItem>,
    seen_ids: &mut std::collections::HashSet<String>,
) {
    if !cat_dir.is_dir() {
        return;
    }

    // 1. Check flat installation (e.g. C:\DevStack\bin\git\cmd\git.exe or composer\composer.phar)
    if let Some((exe, bin_dir)) = find_exe_in_dir(cat_dir, kind) {
        let id = cat_dir.to_string_lossy().to_string();
        if seen_ids.insert(id.clone()) {
            let name = cat_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(kind)
                .to_string();
            out.push(BinaryItem {
                id,
                kind: kind.to_string(),
                name: name.clone(),
                version: name,
                source: source.to_string(),
                root_dir: cat_dir.to_string_lossy().to_string(),
                exe_path: exe.to_string_lossy().to_string(),
                bin_dir: bin_dir.to_string_lossy().to_string(),
                variant: detect_variant(kind, cat_dir, &exe),
            });
        }
    }

    // 2. Check versioned subdirectories (e.g. bin/php/php-8.4.28/php.exe or C:\AppPath\php\8.4.28\php.exe)
    // Skip internal runtime folders like `bin` or `cmd` when scanning subdirectories.
    if let Ok(entries) = fs::read_dir(cat_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let lower_sub = dir_name.to_lowercase();
            if matches!(
                lower_sub.as_str(),
                "bin" | "cmd" | "apache24" | "mingw64" | "usr" | "etc" | "lib" | "tmp" | "dev"
            ) {
                continue;
            }
            if let Some((exe, bin_dir)) = find_exe_in_dir(&path, kind) {
                let id = path.to_string_lossy().to_string();
                if seen_ids.insert(id.clone()) {
                    out.push(BinaryItem {
                        id,
                        kind: kind.to_string(),
                        name: dir_name.clone(),
                        version: clean_version_label(&dir_name, kind),
                        source: source.to_string(),
                        root_dir: path.to_string_lossy().to_string(),
                        exe_path: exe.to_string_lossy().to_string(),
                        bin_dir: bin_dir.to_string_lossy().to_string(),
                        variant: detect_variant(kind, &path, &exe),
                    });
                }
            }
        }
    }
}

pub fn scan_binaries_from_roots(
    portable_bin: &Path,
    external_roots: &[String],
) -> BTreeMap<String, Vec<BinaryItem>> {
    let mut result: BTreeMap<String, Vec<BinaryItem>> = BTreeMap::new();
    let kinds = crate::portable::ALL_BINARY_KINDS;

    for kind in kinds {
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Scan portable bin/<alias>
        for alias in kind_aliases(kind) {
            scan_kind_root(kind, &portable_bin.join(alias), "portable", &mut items, &mut seen);
        }

        // Scan external roots (<root>/<alias> or <root> itself if user linked a category directly)
        for ext in external_roots {
            let ext_path = Path::new(ext);
            if !ext_path.is_dir() {
                continue;
            }
            for alias in kind_aliases(kind) {
                scan_kind_root(kind, &ext_path.join(alias), "external", &mut items, &mut seen);
            }
            // If user linked e.g. C:\AppPath\php directly, check folder name match
            if let Some(folder_name) = ext_path.file_name().and_then(|s| s.to_str()) {
                let lower = folder_name.to_lowercase();
                if kind_aliases(kind).iter().any(|a| *a == lower) {
                    scan_kind_root(kind, ext_path, "external", &mut items, &mut seen);
                }
            }
            // Also support linking a direct version folder (e.g. C:\AppPath\php\8.4.28 or D:\nginx-1.28.0)
            if let Some((exe, bin_dir)) = find_exe_in_dir(ext_path, kind) {
                let id = ext_path.to_string_lossy().to_string();
                if seen.insert(id.clone()) {
                    let dir_name = ext_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(kind)
                        .to_string();
                    items.push(BinaryItem {
                        id,
                        kind: kind.to_string(),
                        name: dir_name.clone(),
                        version: clean_version_label(&dir_name, kind),
                        source: "external".to_string(),
                        root_dir: ext_path.to_string_lossy().to_string(),
                        exe_path: exe.to_string_lossy().to_string(),
                        bin_dir: bin_dir.to_string_lossy().to_string(),
                        variant: detect_variant(kind, ext_path, &exe),
                    });
                }
            }
        }

        // Sort portable first, then newest/alphabetical descending
        items.sort_by(|a, b| {
            let src_ord = (a.source != "portable").cmp(&(b.source != "portable"));
            src_ord.then_with(|| b.version.cmp(&a.version))
        });

        result.insert(kind.to_string(), items);
    }

    result
}

pub fn detect_db_clients(portable_root: &Path, external_roots: &[String]) -> Vec<ExternalDbClient> {
    let mut clients = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut search_candidates: Vec<(String, PathBuf)> = vec![
        (
            "HeidiSQL".into(),
            portable_root.join("usr").join("heidisql").join("heidisql.exe"),
        ),
        (
            "HeidiSQL".into(),
            PathBuf::from(r"C:\Program Files\HeidiSQL\heidisql.exe"),
        ),
        (
            "HeidiSQL".into(),
            PathBuf::from(r"C:\Program Files (x86)\HeidiSQL\heidisql.exe"),
        ),
        (
            "DBeaver".into(),
            PathBuf::from(r"C:\Program Files\DBeaver\dbeaver.exe"),
        ),
        (
            "TablePlus".into(),
            PathBuf::from(r"C:\Program Files\TablePlus\TablePlus.exe"),
        ),
    ];

    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        search_candidates.push((
            "DBeaver".into(),
            PathBuf::from(&local_app).join("DBeaver").join("dbeaver.exe"),
        ));
        search_candidates.push((
            "TablePlus".into(),
            PathBuf::from(&local_app).join("TablePlus").join("TablePlus.exe"),
        ));
    }

    for ext in external_roots {
        let p = Path::new(ext);
        search_candidates.push(("HeidiSQL".into(), p.join("heidisql").join("heidisql.exe")));
        search_candidates.push(("DBeaver".into(), p.join("dbeaver").join("dbeaver.exe")));
    }

    for (name, path) in search_candidates {
        if path.is_file() {
            let exe_str = path.to_string_lossy().to_string();
            if seen.insert(exe_str.to_lowercase()) {
                clients.push(ExternalDbClient {
                    name,
                    exe_path: exe_str,
                });
            }
        }
    }

    clients
}

pub fn scan_inventory(paths: &PortablePaths, config: &mut AppConfig) -> (Inventory, bool) {
    let binaries = scan_binaries_from_roots(&paths.bin, &config.external_bin_roots);
    let mut config_changed = false;

    // Auto-select first discovered binary if active selection is missing or no longer exists
    for (kind, items) in &binaries {
        let current = config.active_versions.get(kind);
        let is_valid = current
            .map(|id| items.iter().any(|item| &item.id == id))
            .unwrap_or(false);

        if !is_valid {
            if let Some(first) = items.first() {
                config.active_versions.insert(kind.clone(), first.id.clone());
                config_changed = true;
            } else if current.is_some() {
                config.active_versions.remove(kind);
                config_changed = true;
            }
        }
    }

    let detected_db_clients = detect_db_clients(&paths.root, &config.external_bin_roots);

    let inventory = Inventory {
        binaries,
        active_versions: config.active_versions.clone(),
        detected_db_clients,
    };

    (inventory, config_changed)
}

pub fn get_active_binary<'a>(
    inventory: &'a Inventory,
    config: &AppConfig,
    kind: &str,
) -> Option<&'a BinaryItem> {
    let active_id = config.active_versions.get(kind)?;
    inventory
        .binaries
        .get(kind)?
        .iter()
        .find(|item| &item.id == active_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_portable_and_external_layouts() {
        let base = std::env::temp_dir().join(format!("envstack_scanner_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let portable_bin = base.join("portable_bin");
        let ext_bin = base.join("ext_bin");

        // Portable versioned PHP
        let php_dir = portable_bin.join("php").join("php-8.4.28");
        fs::create_dir_all(&php_dir).unwrap();
        fs::write(php_dir.join("php.exe"), b"").unwrap();
        fs::write(php_dir.join("php8apache2_4.dll"), b"").unwrap();

        // External flat Git
        let git_cmd = ext_bin.join("git").join("cmd");
        fs::create_dir_all(&git_cmd).unwrap();
        fs::write(git_cmd.join("git.exe"), b"").unwrap();

        // External direct version folder (e.g. ext_nginx/nginx-1.28.0/nginx.exe)
        let direct_nginx = base.join("nginx-1.28.0");
        fs::create_dir_all(&direct_nginx).unwrap();
        fs::write(direct_nginx.join("nginx.exe"), b"").unwrap();

        let map = scan_binaries_from_roots(
            &portable_bin,
            &[
                ext_bin.to_string_lossy().to_string(),
                direct_nginx.to_string_lossy().to_string(),
            ],
        );
        let phps = map.get("php").unwrap();
        assert_eq!(phps.len(), 1);
        assert_eq!(phps[0].source, "portable");
        assert_eq!(phps[0].version, "8.4.28");
        assert_eq!(phps[0].variant.as_deref(), Some("TS"));

        let gits = map.get("git").unwrap();
        assert_eq!(gits.len(), 1);
        assert_eq!(gits[0].source, "external");

        let nginxs = map.get("nginx").unwrap();
        assert_eq!(nginxs.len(), 1);
        assert_eq!(nginxs[0].source, "external");
        assert_eq!(nginxs[0].version, "1.28.0");

        let _ = fs::remove_dir_all(&base);
    }
}
