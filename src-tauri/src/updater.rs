use crate::portable::{APP_VERSION, PortablePaths};
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// "up_to_date" | "update_available" | "no_releases" | "error"
    pub status: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_name: Option<String>,
    pub release_notes: Option<String>,
    pub release_url: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub published_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    assets: Vec<GhAsset>,
}

pub fn parse_semver(raw: &str) -> Option<Version> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    Version::parse(trimmed).ok()
}

pub fn compare_versions(current: &str, latest_tag: &str) -> Option<bool> {
    let cur = parse_semver(current)?;
    let lat = parse_semver(latest_tag)?;
    Some(lat > cur)
}

fn pick_windows_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            (n.ends_with(".exe") || n.ends_with(".zip"))
                && (n.contains("portable") || n.contains("win") || n.contains("x64"))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_lowercase().ends_with(".exe"))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_lowercase().ends_with(".zip"))
        })
}

pub async fn check_for_updates(repo: &str) -> UpdateCheckResult {
    let clean_repo = repo.trim().trim_matches('/');
    let release_page = format!("https://github.com/{clean_repo}/releases");
    let api_url = format!("https://api.github.com/repos/{clean_repo}/releases/latest");

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return UpdateCheckResult {
                status: "error".into(),
                current_version: APP_VERSION.into(),
                latest_version: None,
                release_name: None,
                release_notes: None,
                release_url: release_page,
                asset_name: None,
                asset_url: None,
                published_at: None,
                error: Some(e.to_string()),
            };
        }
    };

    let resp = client
        .get(&api_url)
        .header("User-Agent", format!("EnvStackUI/{APP_VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;

    let response = match resp {
        Ok(r) => r,
        Err(e) => {
            return UpdateCheckResult {
                status: "error".into(),
                current_version: APP_VERSION.into(),
                latest_version: None,
                release_name: None,
                release_notes: None,
                release_url: release_page,
                asset_name: None,
                asset_url: None,
                published_at: None,
                error: Some(e.to_string()),
            };
        }
    };

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return UpdateCheckResult {
            status: "no_releases".into(),
            current_version: APP_VERSION.into(),
            latest_version: None,
            release_name: None,
            release_notes: None,
            release_url: release_page,
            asset_name: None,
            asset_url: None,
            published_at: None,
            error: None,
        };
    }

    if !response.status().is_success() {
        return UpdateCheckResult {
            status: "error".into(),
            current_version: APP_VERSION.into(),
            latest_version: None,
            release_name: None,
            release_notes: None,
            release_url: release_page,
            asset_name: None,
            asset_url: None,
            published_at: None,
            error: Some(format!("GitHub API returned HTTP {}", response.status())),
        };
    }

    let rel: GhRelease = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            return UpdateCheckResult {
                status: "error".into(),
                current_version: APP_VERSION.into(),
                latest_version: None,
                release_name: None,
                release_notes: None,
                release_url: release_page,
                asset_name: None,
                asset_url: None,
                published_at: None,
                error: Some(format!("Failed to parse release JSON: {e}")),
            };
        }
    };

    let is_newer = compare_versions(APP_VERSION, &rel.tag_name).unwrap_or(false);
    let best_asset = pick_windows_asset(&rel.assets);

    UpdateCheckResult {
        status: if is_newer {
            "update_available".into()
        } else {
            "up_to_date".into()
        },
        current_version: APP_VERSION.into(),
        latest_version: Some(rel.tag_name.trim_start_matches(['v', 'V']).to_string()),
        release_name: rel.name,
        release_notes: rel.body,
        release_url: rel.html_url,
        asset_name: best_asset.map(|a| a.name.clone()),
        asset_url: best_asset.map(|a| a.browser_download_url.clone()),
        published_at: rel.published_at,
        error: None,
    }
}

pub async fn download_update(
    paths: &PortablePaths,
    asset_url: &str,
    asset_name: &str,
    app: Option<&AppHandle>,
) -> Result<String> {
    let safe_name = std::path::Path::new(asset_name)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Invalid asset name"))?;

    fs::create_dir_all(&paths.tmp_updates)?;
    let dest = paths.tmp_updates.join(safe_name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let resp = client
        .get(asset_url)
        .header("User-Agent", format!("EnvStackUI/{APP_VERSION}"))
        .send()
        .await?
        .error_for_status()
        .context("Failed to download update asset")?;

    let total = resp.content_length();
    let mut stream = resp.bytes_stream();
    let mut file = File::create(&dest)?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        file.write_all(&bytes)?;
        downloaded += bytes.len() as u64;
        if let Some(h) = app {
            let _ = h.emit(
                "update-download-progress",
                serde_json::json!({
                    "downloaded": downloaded,
                    "total": total,
                }),
            );
        }
    }

    let _ = Command::new("explorer.exe")
        .arg(format!("/select,{}", dest.to_string_lossy()))
        .spawn();

    Ok(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_v_prefix() {
        assert_eq!(compare_versions("0.1.0", "v0.1.0"), Some(false));
        assert_eq!(compare_versions("0.1.0", "v0.1.1"), Some(true));
        assert_eq!(compare_versions("0.1.0", "V0.2.0"), Some(true));
        assert_eq!(compare_versions("1.0.0", "v0.9.9"), Some(false));
    }
}
