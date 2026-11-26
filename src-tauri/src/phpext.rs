use crate::portable::{AppConfig, PortablePaths};
use crate::scanner::{get_active_binary, Inventory};
use crate::services;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpExtensionItem {
    pub name: String,
    pub dll_name: String,
    pub is_zend: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpExtensionsPayload {
    pub ini_path: String,
    pub php_version: String,
    pub ext_dir: String,
    pub xdebug_available: bool,
    pub xdebug_enabled: bool,
    pub xdebug_port: u16,
    pub extensions: Vec<PhpExtensionItem>,
}

fn is_zend_extension(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "opcache" | "xdebug")
}

fn normalize_ext_token(raw: &str) -> String {
    let no_comment = raw.split([';', '#']).next().unwrap_or(raw).trim();
    let unquoted = no_comment.trim_matches(|c| c == '"' || c == '\'');
    let file_part = unquoted.rsplit(['/', '\\']).next().unwrap_or(unquoted);
    let lower = file_part.trim().to_lowercase();
    let no_ext = lower
        .strip_suffix(".dll")
        .or_else(|| lower.strip_suffix(".so"))
        .unwrap_or(&lower);
    no_ext
        .strip_prefix("php_")
        .unwrap_or(no_ext)
        .to_string()
}

fn parse_ini_extension_line(line: &str) -> Option<(bool, bool, String)> {
    let trimmed = line.trim();
    let (commented, rest) = if let Some(after) = trimmed.strip_prefix(';') {
        (true, after.trim())
    } else {
        (false, trimmed)
    };

    let (is_zend, val) = if let Some(rhs) = rest.strip_prefix("zend_extension") {
        let rhs_trim = rhs.trim_start();
        let after_eq = rhs_trim.strip_prefix('=')?;
        (true, after_eq)
    } else if let Some(rhs) = rest.strip_prefix("extension") {
        let rhs_trim = rhs.trim_start();
        let after_eq = rhs_trim.strip_prefix('=')?;
        (false, after_eq)
    } else {
        return None;
    };

    let token = normalize_ext_token(val);
    if token.is_empty() {
        return None;
    }

    Some((commented, is_zend, token))
}

fn parse_xdebug_port(ini_content: &str) -> u16 {
    for line in ini_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rhs) = trimmed.strip_prefix("xdebug.client_port") {
            if let Some(val) = rhs.trim_start().strip_prefix('=') {
                let clean = val.split([';', '#']).next().unwrap_or(val).trim();
                if let Ok(port) = clean.parse::<u16>() {
                    return port;
                }
            }
        }
    }
    9003
}

pub fn list_php_extensions(
    paths: &PortablePaths,
    config: &AppConfig,
    inventory: &Inventory,
) -> Result<PhpExtensionsPayload> {
    let php_bin = get_active_binary(inventory, config, "php")
        .ok_or_else(|| anyhow!("No active PHP binary installed or selected"))?;
    let php_dir = Path::new(&php_bin.root_dir);
    let ext_dir = php_dir.join("ext");
    let ini_path = services::ensure_php_ini(php_dir, paths, config)?;
    let ini_content = fs::read_to_string(&ini_path)
        .with_context(|| format!("Failed to read {}", ini_path.display()))?;

    build_payload_from_parts(&php_bin.version, &ext_dir, &ini_path, &ini_content)
}

pub fn set_php_extension_enabled(
    paths: &PortablePaths,
    config: &AppConfig,
    inventory: &Inventory,
    name: &str,
    enabled: bool,
) -> Result<PhpExtensionsPayload> {
    let php_bin = get_active_binary(inventory, config, "php")
        .ok_or_else(|| anyhow!("No active PHP binary installed or selected"))?;
    let php_dir = Path::new(&php_bin.root_dir);
    let ext_dir = php_dir.join("ext");
    let ini_path = services::ensure_php_ini(php_dir, paths, config)?;
    let ini_content = fs::read_to_string(&ini_path)
        .with_context(|| format!("Failed to read {}", ini_path.display()))?;

    let target = normalize_ext_token(name);
    if target.is_empty() {
        return Err(anyhow!("Invalid extension name"));
    }

    let zend = is_zend_extension(&target);
    let directive = if zend { "zend_extension" } else { "extension" };
    let canonical_line = if zend {
        format!("{directive}=php_{target}.dll")
    } else {
        format!("{directive}={target}")
    };

    let mut out_lines = Vec::new();
    let mut found = false;
    let mut has_xdebug_mode = false;

    for line in ini_content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(';')
            && trimmed.starts_with("xdebug.mode")
            && trimmed.contains('=')
        {
            has_xdebug_mode = true;
        }

        if let Some((_commented, _is_zend, token)) = parse_ini_extension_line(line) {
            if token == target {
                if enabled {
                    if !found {
                        out_lines.push(canonical_line.clone());
                        found = true;
                    }
                    continue;
                } else {
                    out_lines.push(format!(";{canonical_line}"));
                    found = true;
                    continue;
                }
            }
        }
        out_lines.push(line.to_string());
    }

    if enabled && !found {
        out_lines.push(canonical_line);
    }

    if enabled && target == "xdebug" && !has_xdebug_mode {
        out_lines.push(String::new());
        out_lines.push("[xdebug]".to_string());
        out_lines.push("xdebug.mode=debug,develop".to_string());
        out_lines.push("xdebug.start_with_request=yes".to_string());
        out_lines.push("xdebug.client_host=127.0.0.1".to_string());
        out_lines.push("xdebug.client_port=9003".to_string());
    }

    let mut next_ini = out_lines.join("\n");
    if !next_ini.ends_with('\n') {
        next_ini.push('\n');
    }
    fs::write(&ini_path, &next_ini)?;

    build_payload_from_parts(&php_bin.version, &ext_dir, &ini_path, &next_ini)
}

fn build_payload_from_parts(
    php_version: &str,
    ext_dir: &Path,
    ini_path: &Path,
    ini_content: &str,
) -> Result<PhpExtensionsPayload> {
    let mut active_set = HashSet::new();
    for line in ini_content.lines() {
        if let Some((commented, _is_zend, token)) = parse_ini_extension_line(line) {
            if !commented {
                active_set.insert(token);
            }
        }
    }

    let mut extensions = Vec::new();
    let mut seen_names = HashSet::new();

    if ext_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(ext_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                let lower = fname.to_lowercase();
                if !lower.ends_with(".dll") {
                    continue;
                }
                let ext_name = normalize_ext_token(fname);
                if ext_name.is_empty() || !seen_names.insert(ext_name.clone()) {
                    continue;
                }
                let is_zend = is_zend_extension(&ext_name);
                let enabled = active_set.contains(&ext_name);
                extensions.push(PhpExtensionItem {
                    name: ext_name,
                    dll_name: fname.to_string(),
                    is_zend,
                    enabled,
                });
            }
        }
    }

    extensions.sort_by(|a, b| a.name.cmp(&b.name));

    let xdebug_item = extensions.iter().find(|e| e.name == "xdebug");
    let xdebug_available = xdebug_item.is_some();
    let xdebug_enabled = xdebug_item.map(|e| e.enabled).unwrap_or(false);
    let xdebug_port = parse_xdebug_port(ini_content);

    Ok(PhpExtensionsPayload {
        ini_path: ini_path.to_string_lossy().to_string(),
        php_version: php_version.to_string(),
        ext_dir: ext_dir.to_string_lossy().to_string(),
        xdebug_available,
        xdebug_enabled,
        xdebug_port,
        extensions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::BinaryItem;
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn toggles_regular_and_xdebug_extensions_cleanly() {
        let base = std::env::temp_dir().join(format!("envstack_phpext_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        paths.ensure_dirs().unwrap();

        let php_dir = paths.bin.join("php").join("php-8.4.28");
        let ext_dir = php_dir.join("ext");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(php_dir.join("php.exe"), b"").unwrap();
        fs::write(ext_dir.join("php_curl.dll"), b"").unwrap();
        fs::write(ext_dir.join("php_gd.dll"), b"").unwrap();
        fs::write(ext_dir.join("php_xdebug.dll"), b"").unwrap();
        fs::write(ext_dir.join("php_opcache.dll"), b"").unwrap();

        let mut cfg = AppConfig::default();
        let mut bins = BTreeMap::new();
        let php_id = php_dir.to_string_lossy().to_string();
        cfg.active_versions.insert("php".into(), php_id.clone());
        bins.insert(
            "php".into(),
            vec![BinaryItem {
                id: php_id,
                kind: "php".into(),
                name: "php-8.4.28".into(),
                version: "8.4.28".into(),
                source: "portable".into(),
                root_dir: php_dir.to_string_lossy().to_string(),
                exe_path: php_dir.join("php.exe").to_string_lossy().to_string(),
                bin_dir: php_dir.to_string_lossy().to_string(),
                variant: Some("NTS".into()),
            }],
        );
        let inv = Inventory {
            binaries: bins,
            active_versions: HashMap::new(),
            detected_db_clients: Vec::new(),
        };

        let listed = list_php_extensions(&paths, &cfg, &inv).unwrap();
        assert!(listed.xdebug_available);
        assert!(!listed.xdebug_enabled);
        assert!(listed.extensions.iter().any(|e| e.name == "gd" && e.enabled));

        // Disable gd
        let after_disable_gd = set_php_extension_enabled(&paths, &cfg, &inv, "gd", false).unwrap();
        assert!(after_disable_gd
            .extensions
            .iter()
            .any(|e| e.name == "gd" && !e.enabled));

        // Enable xdebug
        let after_enable_xdebug =
            set_php_extension_enabled(&paths, &cfg, &inv, "xdebug", true).unwrap();
        assert!(after_enable_xdebug.xdebug_enabled);
        let ini_text = fs::read_to_string(&after_enable_xdebug.ini_path).unwrap();
        assert!(ini_text.contains("zend_extension=php_xdebug.dll"));
        assert!(ini_text.contains("xdebug.client_port=9003"));

        // Disable xdebug
        let after_disable_xdebug =
            set_php_extension_enabled(&paths, &cfg, &inv, "xdebug", false).unwrap();
        assert!(!after_disable_xdebug.xdebug_enabled);

        let _ = fs::remove_dir_all(&base);
    }
}
