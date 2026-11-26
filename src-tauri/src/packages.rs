use crate::portable::{PortablePaths, ALL_BINARY_KINDS};
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use futures_util::future::join_all;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineVersionItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub variant: Option<String>,
    pub url: String,
    /// "zip" | "tar.gz" | "phar" | "php-single"
    pub archive_type: String,
    pub target_folder: String,
    pub source_label: String,
    pub size_bytes: Option<u64>,
    pub size_label: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub package_id: String,
    pub stage: String, // "downloading" | "extracting" | "done" | "error"
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub eta_seconds: Option<u64>,
    pub message: Option<String>,
}

pub const CACHE_TTL_SECS: i64 = 86_400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCategory {
    pub fetched_at: i64,
    pub items: Vec<OnlineVersionItem>,
}

pub fn cache_file_path(paths: &PortablePaths) -> PathBuf {
    paths.tmp.join("packages_cache.json")
}

pub fn load_online_cache(paths: &PortablePaths) -> HashMap<String, CachedCategory> {
    let path = cache_file_path(paths);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, CachedCategory>>(&raw) {
            return map;
        }
    }
    HashMap::new()
}

pub fn save_online_cache(
    paths: &PortablePaths,
    cache: &HashMap<String, CachedCategory>,
) -> Result<()> {
    fs::create_dir_all(&paths.tmp)?;
    let raw = serde_json::to_string_pretty(cache)?;
    fs::write(cache_file_path(paths), raw)?;
    Ok(())
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn normalize_php_size(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    if let Some(num_str) = lower.strip_suffix("mb") {
        if let Ok(val) = num_str.trim().parse::<f64>() {
            return Some(format!("{val:.1} MB"));
        }
    }
    if let Some(num_str) = lower.strip_suffix("kb") {
        if let Ok(val) = num_str.trim().parse::<f64>() {
            return Some(format!("{val:.0} KB"));
        }
    }
    Some(s.to_string())
}

fn emit_progress_full(
    app: Option<&AppHandle>,
    package_id: &str,
    stage: &str,
    downloaded: u64,
    total: Option<u64>,
    speed_bps: u64,
    eta_seconds: Option<u64>,
    message: Option<String>,
) {
    if let Some(h) = app {
        let _ = h.emit(
            "download-progress",
            DownloadProgress {
                package_id: package_id.to_string(),
                stage: stage.to_string(),
                downloaded,
                total,
                speed_bps,
                eta_seconds,
                message,
            },
        );
    }
}

fn emit_progress(
    app: Option<&AppHandle>,
    package_id: &str,
    stage: &str,
    downloaded: u64,
    total: Option<u64>,
    message: Option<String>,
) {
    emit_progress_full(app, package_id, stage, downloaded, total, 0, None, message);
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
pub struct NodeIndexEntry {
    pub version: String,
    pub files: Vec<String>,
    pub lts: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GoReleaseFile {
    filename: String,
    os: String,
    arch: String,
    kind: String,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GoRelease {
    version: String,
    stable: bool,
    files: Vec<GoReleaseFile>,
}

#[derive(Debug, Deserialize)]
struct MariaMajorEntry {
    release_id: String,
    release_status: String,
}

#[derive(Debug, Deserialize)]
struct MariaMajorIndex {
    major_releases: Vec<MariaMajorEntry>,
}

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

fn parse_version_tuple(raw: &str) -> (u32, u32, u32, bool) {
    let cleaned = raw.trim().trim_start_matches(['v', 'V']);
    let is_prerelease =
        cleaned.contains('-') || cleaned.chars().any(|c| c.is_ascii_alphabetic());
    let numeric_part: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = numeric_part.split('.');
    let maj = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let min = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let pat = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (maj, min, pat, !is_prerelease)
}

pub fn parse_node_index_entries(entries: Vec<NodeIndexEntry>) -> Vec<OnlineVersionItem> {
    let mut per_major_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut items = Vec::new();

    for entry in entries {
        if !entry.files.iter().any(|f| f == "win-x64-zip") {
            continue;
        }
        let ver_lower = entry.version.to_lowercase();
        if ver_lower.contains("alpha")
            || ver_lower.contains("beta")
            || ver_lower.contains("rc")
            || ver_lower.contains("nightly")
        {
            continue;
        }
        let (maj, _, _, _) = parse_version_tuple(&entry.version);
        if maj == 0 {
            continue;
        }

        if !per_major_counts.contains_key(&maj) && per_major_counts.len() >= 6 {
            break;
        }
        let count = per_major_counts.entry(maj).or_insert(0);
        if *count >= 3 {
            continue;
        }
        *count += 1;

        let lts_label = entry.lts.as_str().map(|codename| format!("LTS ({codename})"));
        let tag = if lts_label.is_some() { "lts" } else { "current" };
        let ver_clean = entry.version.trim_start_matches('v').to_string();
        let folder = format!("node-{}-win-x64", entry.version);

        items.push(OnlineVersionItem {
            id: format!("node-{}", entry.version),
            kind: "node".into(),
            name: format!("Node.js {}", entry.version),
            version: ver_clean,
            variant: lts_label,
            url: format!("https://nodejs.org/dist/{}/{}.zip", entry.version, folder),
            archive_type: "zip".into(),
            target_folder: folder,
            source_label: "nodejs.org".into(),
            size_bytes: None,
            size_label: None,
            tags: vec![tag.into()],
        });
    }

    items
}

pub fn parse_php_releases_json(raw: &serde_json::Value) -> Vec<OnlineVersionItem> {
    let Some(obj) = raw.as_object() else {
        return Vec::new();
    };

    let mut branches: Vec<(&String, &serde_json::Value)> = obj.iter().collect();
    branches.sort_by(|a, b| {
        let ta = parse_version_tuple(a.0);
        let tb = parse_version_tuple(b.0);
        tb.0.cmp(&ta.0).then(tb.1.cmp(&ta.1))
    });

    let mut items = Vec::new();
    for (_branch, branch_val) in branches {
        let Some(branch_obj) = branch_val.as_object() else {
            continue;
        };
        let full_ver = branch_obj
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        for (build_key, build_val) in branch_obj {
            if !build_key.ends_with("-x64") {
                continue;
            }
            let variant = if build_key.starts_with("nts-") {
                "NTS"
            } else if build_key.starts_with("ts-") {
                "TS"
            } else {
                continue;
            };

            let Some(zip_obj) = build_val.get("zip") else {
                continue;
            };
            let Some(zip_path) = zip_obj.get("path").and_then(|p| p.as_str()) else {
                continue;
            };

            let size_label = normalize_php_size(zip_obj.get("size").and_then(|s| s.as_str()));
            let folder = zip_path.trim_end_matches(".zip").to_string();
            let ver_display = if full_ver.is_empty() {
                folder.clone()
            } else {
                full_ver.clone()
            };

            items.push(OnlineVersionItem {
                id: format!("php-{ver_display}-{}", variant.to_lowercase()),
                kind: "php".into(),
                name: format!("PHP {ver_display} {variant} (x64)"),
                version: ver_display,
                variant: Some(variant.to_string()),
                url: format!("https://windows.php.net/downloads/releases/{zip_path}"),
                archive_type: "zip".into(),
                target_folder: folder,
                source_label: "windows.php.net".into(),
                size_bytes: None,
                size_label,
                tags: vec![variant.to_lowercase()],
            });
        }
    }

    items
}

pub fn parse_python_standalone_assets(
    assets: &[(&str, &str, Option<u64>)],
) -> Vec<OnlineVersionItem> {
    let suffix = "-x86_64-pc-windows-msvc-install_only.tar.gz";
    let mut items = Vec::new();

    for &(name, url, size) in assets {
        if !name.starts_with("cpython-") || !name.ends_with(suffix) {
            continue;
        }
        let rest = &name["cpython-".len()..name.len() - suffix.len()];
        let (ver, build_tag) = match rest.split_once('+') {
            Some((v, t)) => (v.to_string(), Some(t.to_string())),
            None => (rest.to_string(), None),
        };

        let (_, _, _, is_stable) = parse_version_tuple(&ver);
        let tag = if is_stable { "stable" } else { "prerelease" };

        items.push(OnlineVersionItem {
            id: format!("python-{ver}"),
            kind: "python".into(),
            name: format!("Python {ver} (Standalone x64)"),
            version: ver.clone(),
            variant: build_tag.map(|t| format!("install_only+{t}")),
            url: url.to_string(),
            archive_type: "tar.gz".into(),
            target_folder: format!("python-{ver}"),
            source_label: "astral-sh/python-build-standalone".into(),
            size_bytes: size,
            size_label: size.map(format_bytes),
            tags: vec![tag.into()],
        });
    }

    items.sort_by(|a, b| {
        let ta = parse_version_tuple(&a.version);
        let tb = parse_version_tuple(&b.version);
        tb.3.cmp(&ta.3)
            .then(tb.0.cmp(&ta.0))
            .then(tb.1.cmp(&ta.1))
            .then(tb.2.cmp(&ta.2))
    });

    items
}

pub fn parse_nginx_html(html: &str) -> Vec<OnlineVersionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let bytes = html.as_bytes();
    let mut idx = 0;

    while let Some(pos) = html[idx..].find("nginx-") {
        let abs = idx + pos;
        let rest = &html[abs..];
        if let Some(end) = rest.find(".zip") {
            if end < 30 {
                let candidate = &rest[..end + 4];
                let ver = &candidate["nginx-".len()..candidate.len() - 4];
                let valid = !ver.is_empty()
                    && ver.split('.').count() == 3
                    && ver.chars().all(|c| c.is_ascii_digit() || c == '.');
                if valid && seen.insert(ver.to_string()) {
                    let (_, minor, _, _) = parse_version_tuple(ver);
                    let is_even = minor % 2 == 0;
                    let channel = if is_even { "Stable" } else { "Mainline" };
                    items.push(OnlineVersionItem {
                        id: format!("nginx-{ver}"),
                        kind: "nginx".into(),
                        name: format!("Nginx {ver}"),
                        version: ver.to_string(),
                        variant: Some(channel.into()),
                        url: format!("https://nginx.org/download/nginx-{ver}.zip"),
                        archive_type: "zip".into(),
                        target_folder: format!("nginx-{ver}"),
                        source_label: "nginx.org".into(),
                        size_bytes: None,
                        size_label: None,
                        tags: vec![channel.to_lowercase()],
                    });
                    if items.len() >= 12 {
                        break;
                    }
                }
            }
        }
        idx = (abs + 6).min(bytes.len());
    }

    items
}

pub fn parse_apache_html(html: &str) -> Vec<OnlineVersionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let mut idx = 0;

    while let Some(pos) = html[idx..].find("/download/") {
        let abs = idx + pos;
        let rest = &html[abs..];
        if let Some(end) = rest.find(".zip") {
            if end < 100 {
                let rel = &rest[..end + 4];
                if rel.contains("/binaries/httpd-")
                    && rel.to_lowercase().contains("win64")
                    && !rel.contains('"')
                    && !rel.contains('\'')
                    && seen.insert(rel.to_string())
                {
                    let fname = rel.rsplit('/').next().unwrap_or("httpd.zip");
                    let folder = fname.trim_end_matches(".zip").to_string();
                    let ver = folder
                        .strip_prefix("httpd-")
                        .and_then(|s| s.split('-').next())
                        .unwrap_or(&folder)
                        .to_string();

                    items.push(OnlineVersionItem {
                        id: folder.clone(),
                        kind: "apache".into(),
                        name: format!("Apache {folder}"),
                        version: ver,
                        variant: Some("Win64".into()),
                        url: format!("https://www.apachelounge.com{rel}"),
                        archive_type: "zip".into(),
                        target_folder: folder,
                        source_label: "apachelounge.com".into(),
                        size_bytes: None,
                        size_label: None,
                        tags: vec!["win64".into()],
                    });
                    if items.len() >= 8 {
                        break;
                    }
                }
            }
        }
        idx = (abs + 10).min(html.len());
    }

    items
}

async fn fill_missing_sizes(items: &mut [OnlineVersionItem]) {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    else {
        return;
    };

    let indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.size_label.is_none())
        .map(|(idx, _)| idx)
        .collect();

    if indices.is_empty() {
        return;
    }

    let futures = indices.iter().map(|&idx| {
        let c = &client;
        let url = items[idx].url.clone();
        async move {
            let len = match c
                .head(&url)
                .header("User-Agent", concat!("EnvStackUI/", env!("CARGO_PKG_VERSION")))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => resp.content_length(),
                _ => None,
            };
            (idx, len)
        }
    });

    let results = join_all(futures).await;
    for (idx, len) in results {
        if let Some(bytes) = len {
            if bytes > 0 {
                items[idx].size_bytes = Some(bytes);
                items[idx].size_label = Some(format_bytes(bytes));
            }
        }
    }
}

pub async fn fetch_online_versions(kind: &str) -> Result<Vec<OnlineVersionItem>> {
    let client = http_client()?;
    let ua = concat!("EnvStackUI/", env!("CARGO_PKG_VERSION"));

    let mut items = match kind {
        "node" => {
            let entries: Vec<NodeIndexEntry> = client
                .get("https://nodejs.org/dist/index.json")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            parse_node_index_entries(entries)
        }
        "php" => {
            let raw: serde_json::Value = client
                .get("https://windows.php.net/downloads/releases/releases.json")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            parse_php_releases_json(&raw)
        }
        "python" => {
            let rel: GhRelease = client
                .get("https://api.github.com/repos/astral-sh/python-build-standalone/releases/latest")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let tuples: Vec<(&str, &str, Option<u64>)> = rel
                .assets
                .iter()
                .map(|a| (a.name.as_str(), a.browser_download_url.as_str(), a.size))
                .collect();
            parse_python_standalone_assets(&tuples)
        }
        "nginx" => {
            let html = client
                .get("https://nginx.org/en/download.html")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            parse_nginx_html(&html)
        }
        "apache" => {
            let html = client
                .get("https://www.apachelounge.com/download/")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            parse_apache_html(&html)
        }
        "mysql" => {
            let index: MariaMajorIndex = client
                .get("https://downloads.mariadb.org/rest-api/mariadb/")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            let stable_majors: Vec<String> = index
                .major_releases
                .into_iter()
                .filter(|m| m.release_status.eq_ignore_ascii_case("Stable"))
                .take(4)
                .map(|m| m.release_id)
                .collect();

            let mut list = Vec::new();
            for major in stable_majors {
                let url = format!("https://downloads.mariadb.org/rest-api/mariadb/{major}/");
                let Ok(resp) = client.get(&url).header("User-Agent", ua).send().await else {
                    continue;
                };
                let Ok(val) = resp.json::<serde_json::Value>().await else {
                    continue;
                };
                let Some(releases) = val.get("releases").and_then(|r| r.as_object()) else {
                    continue;
                };
                if let Some((rel_ver, rel_data)) = releases.iter().next() {
                    if let Some(files) = rel_data.get("files").and_then(|f| f.as_array()) {
                        let win_zip = files.iter().find(|f| {
                            f.get("os").and_then(|v| v.as_str()) == Some("Windows")
                                && f.get("cpu").and_then(|v| v.as_str()) == Some("x86_64")
                                && f.get("package_type").and_then(|v| v.as_str()) == Some("ZIP file")
                        });
                        if let Some(file_obj) = win_zip {
                            if let Some(dl_url) =
                                file_obj.get("file_download_url").and_then(|v| v.as_str())
                            {
                                let https_url = dl_url.replacen("http://", "https://", 1);
                                let folder = format!("mariadb-{rel_ver}-winx64");
                                list.push(OnlineVersionItem {
                                    id: format!("mariadb-{rel_ver}"),
                                    kind: "mysql".into(),
                                    name: format!("MariaDB {rel_ver} (x64)"),
                                    version: rel_ver.clone(),
                                    variant: Some("MariaDB Stable".into()),
                                    url: https_url,
                                    archive_type: "zip".into(),
                                    target_folder: folder,
                                    source_label: "mariadb.org".into(),
                                    size_bytes: None,
                                    size_label: None,
                                    tags: vec!["stable".into()],
                                });
                            }
                        }
                    }
                }
            }
            list
        }
        "go" => {
            let releases: Vec<GoRelease> = client
                .get("https://go.dev/dl/?mode=json")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let mut list = Vec::new();
            for rel in releases {
                if let Some(f) = rel
                    .files
                    .into_iter()
                    .find(|f| f.os == "windows" && f.arch == "amd64" && f.kind == "archive")
                {
                    let ver = rel.version.trim_start_matches("go").to_string();
                    list.push(OnlineVersionItem {
                        id: rel.version.clone(),
                        kind: "go".into(),
                        name: format!("Go {ver} (win-amd64)"),
                        version: ver.clone(),
                        variant: if rel.stable { Some("Stable".into()) } else { None },
                        url: format!("https://go.dev/dl/{}", f.filename),
                        archive_type: "zip".into(),
                        target_folder: format!("go-{ver}"),
                        source_label: "go.dev".into(),
                        size_bytes: f.size,
                        size_label: f.size.map(format_bytes),
                        tags: vec![if rel.stable { "stable".into() } else { "prerelease".into() }],
                    });
                }
            }
            list
        }
        "mailpit" => {
            let rels: Vec<GhRelease> = client
                .get("https://api.github.com/repos/axllent/mailpit/releases?per_page=8")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let mut list = Vec::new();
            for rel in rels {
                if let Some(asset) = rel.assets.into_iter().find(|a| {
                    let n = a.name.to_lowercase();
                    n.contains("windows") && n.contains("amd64") && n.ends_with(".zip")
                }) {
                    let tag = rel.tag_name.unwrap_or_else(|| "latest".into());
                    let ver = tag.trim_start_matches('v').to_string();
                    list.push(OnlineVersionItem {
                        id: format!("mailpit-{ver}"),
                        kind: "mailpit".into(),
                        name: format!("Mailpit {tag}"),
                        version: ver.clone(),
                        variant: None,
                        url: asset.browser_download_url,
                        archive_type: "zip".into(),
                        target_folder: format!("mailpit-{ver}"),
                        source_label: "axllent/mailpit".into(),
                        size_bytes: asset.size,
                        size_label: asset.size.map(format_bytes),
                        tags: vec!["stable".into()],
                    });
                }
            }
            list
        }
        "composer" => vec![
            OnlineVersionItem {
                id: "composer-latest-stable".into(),
                kind: "composer".into(),
                name: "Composer (Latest Stable)".into(),
                version: "latest-stable".into(),
                variant: Some("Stable".into()),
                url: "https://getcomposer.org/download/latest-stable/composer.phar".into(),
                archive_type: "phar".into(),
                target_folder: "latest".into(),
                source_label: "getcomposer.org".into(),
                size_bytes: None,
                size_label: None,
                tags: vec!["stable".into()],
            },
            OnlineVersionItem {
                id: "composer-latest-2.2".into(),
                kind: "composer".into(),
                name: "Composer 2.2 LTS".into(),
                version: "2.2-lts".into(),
                variant: Some("LTS".into()),
                url: "https://getcomposer.org/download/latest-2.2.x/composer.phar".into(),
                archive_type: "phar".into(),
                target_folder: "lts-2.2".into(),
                source_label: "getcomposer.org".into(),
                size_bytes: None,
                size_label: None,
                tags: vec!["lts".into()],
            },
        ],
        "adminer" => {
            let rel: GhRelease = client
                .get("https://api.github.com/repos/vrana/adminer/releases/latest")
                .header("User-Agent", ua)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let tag = rel.tag_name.unwrap_or_else(|| "latest".into());
            let ver = tag.trim_start_matches('v').to_string();
            let picked = rel.assets.into_iter().find(|a| {
                let n = a.name.to_lowercase();
                n.starts_with("adminer-")
                    && n.ends_with(".php")
                    && !n.contains("-en")
                    && !n.contains("mysql")
            });
            let (dl_url, size) = match picked {
                Some(a) => (a.browser_download_url, a.size),
                None => ("https://www.adminer.org/latest.php".to_string(), None),
            };

            vec![OnlineVersionItem {
                id: format!("adminer-{ver}"),
                kind: "adminer".into(),
                name: format!("Adminer {ver} (Dark Theme)"),
                version: ver,
                variant: Some("Single PHP".into()),
                url: dl_url,
                archive_type: "php-single".into(),
                target_folder: "adminer".into(),
                source_label: "vrana/adminer".into(),
                size_bytes: size,
                size_label: size.map(format_bytes),
                tags: vec!["stable".into()],
            }]
        }
        _ => return Err(anyhow!("Unsupported package category '{kind}'")),
    };

    fill_missing_sizes(&mut items).await;
    Ok(items)
}

async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    package_id: &str,
    app: Option<&AppHandle>,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let resp = client
        .get(url)
        .header("User-Agent", concat!("EnvStackUI/", env!("CARGO_PKG_VERSION")))
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("Download failed from {url}"))?;

    let total = resp.content_length();
    let mut stream = resp.bytes_stream();
    let mut file = File::create(dest)?;
    let mut downloaded: u64 = 0;
    let started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(200))
        .unwrap_or_else(std::time::Instant::now);

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        file.write_all(&bytes)?;
        downloaded += bytes.len() as u64;

        if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
            let elapsed_secs = started.elapsed().as_secs_f64().max(0.05);
            let speed_bps = (downloaded as f64 / elapsed_secs) as u64;
            let eta_seconds = total.and_then(|t| {
                if t > downloaded && speed_bps > 0 {
                    Some((t - downloaded) / speed_bps)
                } else {
                    Some(0)
                }
            });
            emit_progress_full(
                app,
                package_id,
                "downloading",
                downloaded,
                total,
                speed_bps,
                eta_seconds,
                None,
            );
            last_emit = std::time::Instant::now();
        }
    }

    let elapsed_secs = started.elapsed().as_secs_f64().max(0.05);
    let speed_bps = (downloaded as f64 / elapsed_secs) as u64;
    emit_progress_full(
        app,
        package_id,
        "downloading",
        downloaded,
        total,
        speed_bps,
        Some(0),
        None,
    );

    Ok(())
}

fn sanitize_relative_path(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(seg) => clean.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(clean)
}

fn is_metadata_root_file(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    !matches!(
        Path::new(&lower).extension().and_then(|s| s.to_str()),
        Some(
            "exe"
                | "dll"
                | "bat"
                | "cmd"
                | "com"
                | "phar"
                | "msi"
                | "ps1"
                | "py"
                | "php"
                | "so"
                | "pyd"
                | "lib"
        )
    )
}

fn resolve_single_top_wrapper(
    top_subdirs: HashSet<String>,
    nested_file_count: usize,
    root_files: &[String],
) -> Option<String> {
    if top_subdirs.len() == 1
        && nested_file_count > 0
        && (root_files.is_empty() || root_files.iter().all(|f| is_metadata_root_file(f)))
    {
        top_subdirs.into_iter().next()
    } else {
        None
    }
}

fn extract_zip_flattened(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut top_subdirs = HashSet::new();
    let mut nested_file_count = 0usize;
    let mut root_files = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let comps: Vec<_> = enclosed.components().collect();
        if let Some(Component::Normal(first)) = comps.first() {
            let first_name = first.to_string_lossy().to_string();
            if comps.len() >= 2 {
                top_subdirs.insert(first_name);
                if !entry.is_dir() {
                    nested_file_count += 1;
                }
            } else if entry.is_dir() {
                top_subdirs.insert(first_name);
            } else {
                root_files.push(first_name);
            }
        }
    }
    let single_top = resolve_single_top_wrapper(top_subdirs, nested_file_count, &root_files);

    fs::create_dir_all(dest_dir)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };

        let rel_path: PathBuf = if let Some(top) = &single_top {
            let Ok(stripped) = enclosed.strip_prefix(top) else {
                continue;
            };
            stripped.to_path_buf()
        } else {
            enclosed
        };

        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let out_path = dest_dir.join(&rel_path);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}

pub fn extract_targz_flattened(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let mut top_subdirs = HashSet::new();
    let mut nested_file_count = 0usize;
    let mut root_files = Vec::new();
    {
        let file = File::open(archive_path)?;
        let gz = GzDecoder::new(file);
        let mut archive = Archive::new(gz);
        for entry_res in archive.entries()? {
            let entry = entry_res?;
            let is_dir = entry.header().entry_type().is_dir();
            let is_file = entry.header().entry_type().is_file();
            if let Ok(path) = entry.path() {
                if let Some(clean) = sanitize_relative_path(&path) {
                    let comps: Vec<_> = clean.components().collect();
                    if let Some(Component::Normal(first)) = comps.first() {
                        let first_name = first.to_string_lossy().to_string();
                        if comps.len() >= 2 {
                            top_subdirs.insert(first_name);
                            if is_file {
                                nested_file_count += 1;
                            }
                        } else if is_dir {
                            top_subdirs.insert(first_name);
                        } else if is_file {
                            root_files.push(first_name);
                        }
                    }
                }
            }
        }
    }

    let single_top = resolve_single_top_wrapper(top_subdirs, nested_file_count, &root_files);

    fs::create_dir_all(dest_dir)?;

    let file = File::open(archive_path)?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);
    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let raw_path = entry.path()?.into_owned();
        let Some(clean) = sanitize_relative_path(&raw_path) else {
            continue;
        };

        let rel_path = if let Some(top) = &single_top {
            let Ok(stripped) = clean.strip_prefix(top) else {
                continue;
            };
            stripped.to_path_buf()
        } else {
            clean
        };

        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let out_path = dest_dir.join(&rel_path);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out_path)?;
        } else if entry.header().entry_type().is_file() {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}

pub async fn install_online_package(
    paths: &PortablePaths,
    item: &OnlineVersionItem,
    app: Option<&AppHandle>,
) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let res = install_online_inner(&client, paths, item, app).await;
    if let Err(ref e) = res {
        emit_progress(app, &item.id, "error", 0, None, Some(format!("{e:#}")));
    }
    res
}

async fn install_online_inner(
    client: &reqwest::Client,
    paths: &PortablePaths,
    item: &OnlineVersionItem,
    app: Option<&AppHandle>,
) -> Result<String> {
    if item.kind == "adminer" || item.archive_type == "php-single" {
        fs::create_dir_all(&paths.usr_adminer)?;
        let index_php = paths.usr_adminer.join("index.php");
        download_to_file(client, &item.url, &index_php, &item.id, app).await?;
        write_adminer_dark_css(&paths.usr_adminer)?;
        emit_progress(app, &item.id, "done", 100, Some(100), None);
        return Ok(paths.usr_adminer.to_string_lossy().to_string());
    }

    let safe_folder = sanitize_folder_name(&item.target_folder);
    let dest_dir = paths.bin.join(&item.kind).join(&safe_folder);
    fs::create_dir_all(&paths.tmp)?;

    if item.archive_type == "phar" {
        fs::create_dir_all(&dest_dir)?;
        let phar_path = dest_dir.join("composer.phar");
        download_to_file(client, &item.url, &phar_path, &item.id, app).await?;
        fs::write(dest_dir.join("composer.bat"), "@php \"%~dp0composer.phar\" %*\r\n")?;
        emit_progress(app, &item.id, "done", 100, Some(100), None);
        return Ok(dest_dir.to_string_lossy().to_string());
    }

    let is_targz = item.archive_type == "tar.gz"
        || item.url.to_lowercase().ends_with(".tar.gz")
        || item.url.to_lowercase().ends_with(".tgz");
    let ext = if is_targz { "tar.gz" } else { "zip" };
    let tmp_archive = paths.tmp.join(format!("{}.{ext}", sanitize_folder_name(&item.id)));

    download_to_file(client, &item.url, &tmp_archive, &item.id, app).await?;

    emit_progress(app, &item.id, "extracting", 0, None, None);
    let archive_clone = tmp_archive.clone();
    let dest_clone = dest_dir.clone();
    tokio::task::spawn_blocking(move || {
        if is_targz {
            extract_targz_flattened(&archive_clone, &dest_clone)
        } else {
            extract_zip_flattened(&archive_clone, &dest_clone)
        }
    })
    .await??;

    let _ = fs::remove_file(&tmp_archive);
    emit_progress(app, &item.id, "done", 100, Some(100), None);
    Ok(dest_dir.to_string_lossy().to_string())
}

fn sanitize_folder_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('.');
    if trimmed.is_empty() {
        "custom-version".to_string()
    } else {
        trimmed.to_string()
    }
}

pub async fn install_custom_archive(
    paths: &PortablePaths,
    kind: &str,
    version_name: &str,
    source: &str,
    is_local_file: bool,
    app: Option<&AppHandle>,
) -> Result<String> {
    if !ALL_BINARY_KINDS.contains(&kind) && kind != "adminer" {
        return Err(anyhow!("Unsupported target category '{kind}'"));
    }

    let progress_id = "custom";
    let src_trimmed = source.trim();
    if src_trimmed.is_empty() {
        return Err(anyhow!("Source URL or file path cannot be empty"));
    }

    let inferred_name = src_trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("custom")
        .split('?')
        .next()
        .unwrap_or("custom")
        .trim_end_matches(".tar.gz")
        .trim_end_matches(".tgz")
        .trim_end_matches(".zip")
        .trim_end_matches(".phar");

    let target_folder = sanitize_folder_name(if version_name.trim().is_empty() {
        inferred_name
    } else {
        version_name
    });

    let lower_src = src_trimmed.to_lowercase();
    let is_targz = lower_src.ends_with(".tar.gz") || lower_src.ends_with(".tgz");
    let is_phar = lower_src.ends_with(".phar");

    if is_local_file {
        let local_path = PathBuf::from(src_trimmed);
        if !local_path.is_file() {
            return Err(anyhow!("Archive file not found: {src_trimmed}"));
        }
        let dest_dir = paths.bin.join(kind).join(&target_folder);
        if is_phar {
            fs::create_dir_all(&dest_dir)?;
            fs::copy(&local_path, dest_dir.join("composer.phar"))?;
            fs::write(dest_dir.join("composer.bat"), "@php \"%~dp0composer.phar\" %*\r\n")?;
        } else {
            emit_progress(app, progress_id, "extracting", 0, None, None);
            let p_clone = local_path.clone();
            let d_clone = dest_dir.clone();
            tokio::task::spawn_blocking(move || {
                if is_targz {
                    extract_targz_flattened(&p_clone, &d_clone)
                } else {
                    extract_zip_flattened(&p_clone, &d_clone)
                }
            })
            .await??;
        }
        emit_progress(app, progress_id, "done", 100, Some(100), None);
        return Ok(dest_dir.to_string_lossy().to_string());
    }

    let item = OnlineVersionItem {
        id: progress_id.to_string(),
        kind: kind.to_string(),
        name: target_folder.clone(),
        version: target_folder.clone(),
        variant: Some("Custom".into()),
        url: src_trimmed.to_string(),
        archive_type: if is_targz {
            "tar.gz".into()
        } else if is_phar {
            "phar".into()
        } else if kind == "adminer" {
            "php-single".into()
        } else {
            "zip".into()
        },
        target_folder,
        source_label: "custom-url".into(),
        size_bytes: None,
        size_label: None,
        tags: vec!["custom".into()],
    };

    install_online_package(paths, &item, app).await
}

pub fn write_adminer_dark_css(adminer_dir: &Path) -> Result<()> {
    let css = r#"/* EnvStackUI Dark Theme for Adminer */
body { background: #21252b; color: #dce0e5; font-family: 'Inter', 'Segoe UI', sans-serif; }
a, a:visited { color: #74ade8; }
#menu { background: #1c2026; border-right: 1px solid #3b414d; }
#menu h1 { background: #282c33; border-bottom: 1px solid #3b414d; color: #dce0e5; }
#content { background: #21252b; }
table { border-color: #3b414d; }
th, thead td { background: #282c33; color: #dce0e5; border-color: #3b414d; }
td { border-color: #2f343e; }
.odd td { background: #242830; }
input, select, textarea { background: #282c33; color: #dce0e5; border: 1px solid #3b414d; border-radius: 4px; padding: 4px 6px; }
input[type=submit], button { background: #74ade8; color: #1a1d23; border: none; font-weight: 600; cursor: pointer; }
.message { background: #263329; border: 1px solid #a1c181; color: #a1c181; }
.error { background: #382528; border: 1px solid #d07277; color: #d07277; }
"#;
    fs::write(adminer_dir.join("adminer.css"), css)?;
    Ok(())
}

pub async fn ensure_adminer(paths: &PortablePaths, app: Option<&AppHandle>) -> Result<()> {
    let index = paths.usr_adminer.join("index.php");
    if !index.is_file() {
        let versions = fetch_online_versions("adminer").await?;
        let latest = versions
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No Adminer release available"))?;
        install_online_package(paths, &latest, app).await?;
    } else if !paths.usr_adminer.join("adminer.css").is_file() {
        write_adminer_dark_css(&paths.usr_adminer)?;
    }
    Ok(())
}

pub fn uninstall_package(paths: &PortablePaths, kind: &str, target_folder: &str) -> Result<()> {
    if kind == "adminer" {
        let idx = paths.usr_adminer.join("index.php");
        let css = paths.usr_adminer.join("adminer.css");
        if idx.is_file() {
            fs::remove_file(&idx)?;
        }
        if css.is_file() {
            let _ = fs::remove_file(&css);
        }
        return Ok(());
    }

    if !ALL_BINARY_KINDS.contains(&kind) {
        return Err(anyhow!("Unsupported package category '{kind}'"));
    }

    let safe_folder = sanitize_folder_name(target_folder);
    let kind_root = paths.bin.join(kind);
    let target_dir = kind_root.join(&safe_folder);
    if !target_dir.is_dir() {
        return Err(anyhow!("Portable package folder not found: {}", target_dir.display()));
    }
    if !target_dir.starts_with(&kind_root) || target_dir == kind_root {
        return Err(anyhow!("Refusing to delete unsafe path"));
    }

    fs::remove_dir_all(&target_dir)
        .with_context(|| format!("Failed to remove {}", target_dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    #[test]
    fn formats_bytes_cleanly() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(512 * 1024), "512 KB");
        assert_eq!(format_bytes(26_214_400), "25.0 MB");
    }

    #[test]
    fn parses_python_standalone_install_only_assets() {
        let assets = vec![
            (
                "cpython-3.13.18+20261124-x86_64-pc-windows-msvc-install_only.tar.gz",
                "https://example.com/3.13.18.tar.gz",
                Some(29_500_000),
            ),
            (
                "cpython-3.14.10+20261124-x86_64-pc-windows-msvc-install_only.tar.gz",
                "https://example.com/3.14.10.tar.gz",
                Some(31_457_280),
            ),
            (
                "cpython-3.16.0a2+20261124-x86_64-pc-windows-msvc-install_only.tar.gz",
                "https://example.com/3.16.0a2.tar.gz",
                Some(30_000_000),
            ),
            (
                "cpython-3.14.10+20261124-x86_64-pc-windows-msvc-full.tar.zst",
                "https://example.com/ignored.tar.zst",
                None,
            ),
        ];

        let parsed = parse_python_standalone_assets(&assets);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].version, "3.14.10");
        assert_eq!(parsed[0].archive_type, "tar.gz");
        assert_eq!(parsed[0].size_label.as_deref(), Some("30.0 MB"));
        assert_eq!(parsed[0].tags, vec!["stable"]);
        assert_eq!(parsed[1].version, "3.13.18");
        assert_eq!(parsed[2].version, "3.16.0a2");
        assert_eq!(parsed[2].tags, vec!["prerelease"]);
    }

    #[test]
    fn parses_php_releases_json_ts_and_nts() {
        let raw = serde_json::json!({
            "8.3": {
                "version": "8.3.36",
                "nts-vs16-x64": { "zip": { "path": "php-8.3.36-nts-Win32-vs16-x64.zip", "size": "28.42MB" } },
                "ts-vs16-x64": { "zip": { "path": "php-8.3.36-Win32-vs16-x64.zip", "size": "29.10MB" } }
            },
            "8.4": {
                "version": "8.4.28",
                "nts-vs17-x64": { "zip": { "path": "php-8.4.28-nts-Win32-vs17-x64.zip", "size": "30.05MB" } }
            }
        });

        let items = parse_php_releases_json(&raw);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].version, "8.4.28");
        assert_eq!(items[0].variant.as_deref(), Some("NTS"));
        assert_eq!(items[0].size_label.as_deref(), Some("30.1 MB"));
        assert_eq!(items[0].tags, vec!["nts"]);
        assert!(items
            .iter()
            .any(|i| i.version == "8.3.36" && i.tags == vec!["ts"]));
    }

    #[test]
    fn tags_nginx_stable_and_mainline() {
        let html = r#"<a href="/download/nginx-1.31.10.zip">nginx-1.31.10.zip</a> <a href="/download/nginx-1.30.8.zip">nginx-1.30.8.zip</a>"#;
        let items = parse_nginx_html(html);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].tags, vec!["mainline"]);
        assert_eq!(items[1].tags, vec!["stable"]);
    }

    #[test]
    fn extracts_targz_and_strips_single_top_wrapper() {
        let base = std::env::temp_dir().join(format!("envstack_targz_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let tgz_path = base.join("python.tar.gz");
        {
            let f = File::create(&tgz_path).unwrap();
            let enc = GzEncoder::new(f, Compression::default());
            let mut builder = tar::Builder::new(enc);

            let data = b"fake-python-exe";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "python/python.exe", &data[..])
                .unwrap();

            let data2 = b"lib";
            let mut header2 = tar::Header::new_gnu();
            header2.set_size(data2.len() as u64);
            header2.set_mode(0o644);
            header2.set_cksum();
            builder
                .append_data(&mut header2, "python/Lib/site.py", &data2[..])
                .unwrap();

            builder.into_inner().unwrap().finish().unwrap();
        }

        let dest = base.join("out");
        extract_targz_flattened(&tgz_path, &dest).unwrap();
        assert!(dest.join("python.exe").is_file());
        assert!(dest.join("Lib").join("site.py").is_file());
        assert!(!dest.join("python").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn extracts_zip_and_unwraps_apache_layout() {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;

        let base = std::env::temp_dir().join(format!("envstack_apache_zip_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let zip_path = base.join("apache.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts = SimpleFileOptions::default();

            writer.start_file("Apache24/bin/httpd.exe", opts).unwrap();
            writer.write_all(b"fake-httpd").unwrap();
            writer.start_file("Apache24/conf/httpd.conf", opts).unwrap();
            writer.write_all(b"ServerRoot .").unwrap();
            writer.start_file("ReadMe.txt", opts).unwrap();
            writer.write_all(b"readme").unwrap();
            writer.start_file("Security.txt", opts).unwrap();
            writer.write_all(b"security").unwrap();
            writer.start_file("-- Win64 VS18  --", opts).unwrap();
            writer.write_all(b"").unwrap();
            writer.finish().unwrap();
        }

        let dest = base.join("out_apache");
        extract_zip_flattened(&zip_path, &dest).unwrap();
        assert!(dest.join("bin").join("httpd.exe").is_file());
        assert!(dest.join("conf").join("httpd.conf").is_file());
        assert!(!dest.join("Apache24").exists());
        assert!(!dest.join("ReadMe.txt").exists());

        // Also ensure archives with root executables stay flat
        let flat_zip_path = base.join("php_flat.zip");
        {
            let file = File::create(&flat_zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            writer.start_file("php.exe", opts).unwrap();
            writer.write_all(b"fake-php").unwrap();
            writer.start_file("ext/php_curl.dll", opts).unwrap();
            writer.write_all(b"fake-ext").unwrap();
            writer.finish().unwrap();
        }

        let flat_dest = base.join("out_php");
        extract_zip_flattened(&flat_zip_path, &flat_dest).unwrap();
        assert!(flat_dest.join("php.exe").is_file());
        assert!(flat_dest.join("ext").join("php_curl.dll").is_file());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn uninstall_package_removes_only_portable_bin_folder() {
        let base = std::env::temp_dir().join(format!("envstack_uninstall_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        paths.ensure_dirs().unwrap();

        let pkg_dir = paths.bin.join("php").join("php-8.3.36-nts");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("php.exe"), b"").unwrap();

        uninstall_package(&paths, "php", "php-8.3.36-nts").unwrap();
        assert!(!pkg_dir.exists());
        assert!(paths.bin.join("php").is_dir());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn persists_online_cache_to_disk() {
        let base = std::env::temp_dir().join(format!("envstack_cache_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let paths = PortablePaths::from_root(base.clone());
        paths.ensure_dirs().unwrap();

        let mut cache = HashMap::new();
        cache.insert(
            "php".to_string(),
            CachedCategory {
                fetched_at: 1_700_000_000,
                items: vec![OnlineVersionItem {
                    id: "php-8.4.7-nts".into(),
                    kind: "php".into(),
                    name: "PHP 8.4.7 NTS".into(),
                    version: "8.4.7".into(),
                    variant: Some("NTS".into()),
                    url: "https://example.com/php.zip".into(),
                    archive_type: "zip".into(),
                    target_folder: "php-8.4.7-nts".into(),
                    source_label: "windows.php.net".into(),
                    size_bytes: Some(1024),
                    size_label: Some("1 KB".into()),
                    tags: vec!["nts".into()],
                }],
            },
        );

        save_online_cache(&paths, &cache).unwrap();
        let loaded = load_online_cache(&paths);
        assert_eq!(loaded.get("php").unwrap().items.len(), 1);
        assert_eq!(loaded.get("php").unwrap().fetched_at, 1_700_000_000);

        let _ = fs::remove_dir_all(&base);
    }
}
