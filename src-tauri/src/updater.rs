use crate::portable::{PortablePaths, APP_VERSION};
use crate::services::is_pid_alive;
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    pub asset_size: Option<u64>,
    pub checksum_url: Option<String>,
    pub published_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
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
            n.ends_with(".exe") && (n.contains("portable") || n.contains("win") || n.contains("x64"))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_lowercase().ends_with(".exe"))
        })
        .or_else(|| {
            assets.iter().find(|a| {
                let n = a.name.to_lowercase();
                n.ends_with(".zip")
                    && (n.contains("portable") || n.contains("win") || n.contains("x64"))
            })
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_lowercase().ends_with(".zip"))
        })
}

fn pick_checksum_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    assets.iter().find(|a| {
        let n = a.name.to_lowercase();
        n == "checksums.txt" || n == "sha256sums.txt" || n.ends_with(".sha256")
    })
}

fn check_error_result(release_page: String, error_msg: String) -> UpdateCheckResult {
    UpdateCheckResult {
        status: "error".into(),
        current_version: APP_VERSION.into(),
        latest_version: None,
        release_name: None,
        release_notes: None,
        release_url: release_page,
        asset_name: None,
        asset_url: None,
        asset_size: None,
        checksum_url: None,
        published_at: None,
        error: Some(error_msg),
    }
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
        Err(e) => return check_error_result(release_page, e.to_string()),
    };

    let response = match client
        .get(&api_url)
        .header("User-Agent", format!("EnvStackUI/{APP_VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return check_error_result(release_page, e.to_string()),
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
            asset_size: None,
            checksum_url: None,
            published_at: None,
            error: None,
        };
    }

    if !response.status().is_success() {
        return check_error_result(
            release_page,
            format!("GitHub API returned HTTP {}", response.status()),
        );
    }

    let rel: GhRelease = match response.json().await {
        Ok(r) => r,
        Err(e) => return check_error_result(release_page, format!("Failed to parse release JSON: {e}")),
    };

    let is_newer = compare_versions(APP_VERSION, &rel.tag_name).unwrap_or(false);
    let best_asset = pick_windows_asset(&rel.assets);
    let checksum_asset = pick_checksum_asset(&rel.assets);

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
        asset_size: best_asset.and_then(|a| a.size),
        checksum_url: checksum_asset.map(|a| a.browser_download_url.clone()),
        published_at: rel.published_at,
        error: None,
    }
}

struct Sha256Hasher {
    state: [u32; 8],
    buffer: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Sha256Hasher {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(Self::K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buffer[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let blk = self.buffer;
                Self::compress(&mut self.state, &blk);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut blk = [0u8; 64];
            blk.copy_from_slice(&data[..64]);
            Self::compress(&mut self.state, &blk);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn finalize(mut self) -> String {
        let bit_len = self.total_len.wrapping_mul(8);
        self.update(&[0x80]);
        while self.buf_len != 56 {
            self.update(&[0u8]);
        }
        self.update(&bit_len.to_be_bytes());
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
}

#[cfg(test)]
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize())
}

pub fn parse_checksum_for_asset(text: &str, asset_name: &str) -> Option<String> {
    let target = asset_name.trim().to_lowercase();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?.to_lowercase();
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if let Some(file_col) = parts.next() {
            let clean_file = file_col
                .trim_start_matches('*')
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(file_col)
                .to_lowercase();
            if clean_file == target {
                return Some(hash);
            }
        } else {
            return Some(hash);
        }
    }
    None
}

pub fn validate_pe_executable(path: &Path, min_bytes: u64) -> Result<()> {
    let meta = fs::metadata(path).with_context(|| format!("Missing {}", path.display()))?;
    if meta.len() < min_bytes {
        return Err(anyhow!(
            "Downloaded binary looks incomplete ({} bytes)",
            meta.len()
        ));
    }
    let mut file = File::open(path)?;
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)?;
    if &magic != b"MZ" {
        return Err(anyhow!("Downloaded file is not a valid Windows executable"));
    }
    Ok(())
}

fn emit_update_progress(
    app: Option<&AppHandle>,
    stage: &str,
    downloaded: u64,
    total: Option<u64>,
    speed_bps: u64,
) {
    if let Some(h) = app {
        let _ = h.emit(
            "update-download-progress",
            serde_json::json!({
                "stage": stage,
                "downloaded": downloaded,
                "total": total,
                "speed_bps": speed_bps,
            }),
        );
    }
}

fn extract_exe_from_zip(zip_path: &Path, out_exe: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        if entry.name().to_lowercase().ends_with(".exe") {
            let mut out = File::create(out_exe)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(anyhow!("No .exe executable found inside update archive"))
}

pub async fn download_and_stage_update(
    paths: &PortablePaths,
    asset_url: &str,
    asset_name: &str,
    checksum_url: Option<&str>,
    app: Option<&AppHandle>,
) -> Result<PathBuf> {
    let safe_name = Path::new(asset_name)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Invalid asset name"))?;

    fs::create_dir_all(&paths.tmp_updates)?;
    let is_zip = safe_name.to_lowercase().ends_with(".zip");
    let raw_dest = paths.tmp_updates.join(if is_zip {
        "staged_download.zip"
    } else {
        "envstack-ui.new.exe"
    });
    let staged_exe = paths.tmp_updates.join("envstack-ui.new.exe");
    let _ = fs::remove_file(&raw_dest);
    let _ = fs::remove_file(&staged_exe);

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
    let mut file = File::create(&raw_dest)?;
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
            let elapsed = started.elapsed().as_secs_f64().max(0.05);
            let speed_bps = (downloaded as f64 / elapsed) as u64;
            emit_update_progress(app, "downloading", downloaded, total, speed_bps);
            last_emit = std::time::Instant::now();
        }
    }
    drop(file);

    let elapsed = started.elapsed().as_secs_f64().max(0.05);
    let speed_bps = (downloaded as f64 / elapsed) as u64;
    emit_update_progress(app, "downloading", downloaded, total, speed_bps);

    if let Some(expected_len) = total {
        if downloaded != expected_len {
            let _ = fs::remove_file(&raw_dest);
            return Err(anyhow!(
                "Incomplete update download ({downloaded} of {expected_len} bytes)"
            ));
        }
    }

    emit_update_progress(app, "verifying", downloaded, total, 0);

    if let Some(cs_url) = checksum_url {
        if let Ok(cs_resp) = client
            .get(cs_url)
            .header("User-Agent", format!("EnvStackUI/{APP_VERSION}"))
            .send()
            .await
        {
            if cs_resp.status().is_success() {
                if let Ok(cs_text) = cs_resp.text().await {
                    if let Some(expected_hash) = parse_checksum_for_asset(&cs_text, safe_name) {
                        let raw_clone = raw_dest.clone();
                        let actual_hash =
                            tokio::task::spawn_blocking(move || sha256_file(&raw_clone))
                                .await??;
                        if actual_hash != expected_hash {
                            let _ = fs::remove_file(&raw_dest);
                            return Err(anyhow!(
                                "Update SHA-256 verification failed (expected {expected_hash}, got {actual_hash})"
                            ));
                        }
                    }
                }
            }
        }
    }

    if is_zip {
        let raw_clone = raw_dest.clone();
        let staged_clone = staged_exe.clone();
        tokio::task::spawn_blocking(move || extract_exe_from_zip(&raw_clone, &staged_clone))
            .await??;
        let _ = fs::remove_file(&raw_dest);
    }

    validate_pe_executable(&staged_exe, 1_000_000).inspect_err(|_| {
        let _ = fs::remove_file(&staged_exe);
    })?;

    Ok(staged_exe)
}

pub fn old_backup_path(current_exe: &Path) -> PathBuf {
    let fname = current_exe
        .file_name()
        .map(|s| format!("{}.old", s.to_string_lossy()))
        .unwrap_or_else(|| "envstack-ui.exe.old".to_string());
    current_exe.with_file_name(fname)
}

pub fn swap_executable_files(current_exe: &Path, staged_exe: &Path) -> Result<PathBuf> {
    let old_exe = old_backup_path(current_exe);
    if old_exe.exists() {
        fs::remove_file(&old_exe)
            .with_context(|| format!("Failed to clear old backup {}", old_exe.display()))?;
    }

    fs::rename(current_exe, &old_exe)
        .with_context(|| format!("Failed to move running executable to {}", old_exe.display()))?;

    let move_res = fs::rename(staged_exe, current_exe).or_else(|_| {
        fs::copy(staged_exe, current_exe)?;
        let _ = fs::remove_file(staged_exe);
        Ok::<(), std::io::Error>(())
    });

    if let Err(err) = move_res {
        // Roll back immediately so the user is never left without an executable
        let _ = fs::rename(&old_exe, current_exe);
        return Err(anyhow!(
            "Failed to place new update binary (rolled back cleanly): {err}"
        ));
    }

    Ok(old_exe)
}

pub fn rollback_executable_swap(current_exe: &Path, old_exe: &Path) {
    if old_exe.is_file() {
        let _ = fs::remove_file(current_exe);
        let _ = fs::rename(old_exe, current_exe);
    }
}

pub fn handle_prelaunch_wait() {
    let args: Vec<String> = std::env::args().collect();
    let mut wait_pid: Option<u32> = None;
    for pair in args.windows(2) {
        if pair[0] == "--wait-pid" {
            if let Ok(pid) = pair[1].parse::<u32>() {
                wait_pid = Some(pid);
                break;
            }
        }
    }

    let Some(old_pid) = wait_pid else {
        return;
    };

    for _ in 0..80 {
        if !is_pid_alive(old_pid) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // Allow OS process handle & single-instance mutex to release cleanly
    std::thread::sleep(std::time::Duration::from_millis(250));

    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = old_backup_path(&current_exe);
        for _ in 0..5 {
            if !old_exe.exists() {
                break;
            }
            if fs::remove_file(&old_exe).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    }
}

pub fn cleanup_old_update_files(paths: &PortablePaths) {
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = old_backup_path(&current_exe);
        if old_exe.is_file() {
            let _ = fs::remove_file(old_exe);
        }
    }
    if paths.tmp_updates.is_dir() {
        if let Ok(entries) = fs::read_dir(&paths.tmp_updates) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
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

    #[test]
    fn sha256_bytes_matches_known_vectors() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parses_checksum_file_entries() {
        let text = "\
            # release checksums\n\
            ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *EnvStackUI-v1.0.2-win-x64-portable.exe\n\
            e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  EnvStackUI-v1.0.2-win-x64-portable.zip\n";
        assert_eq!(
            parse_checksum_for_asset(text, "EnvStackUI-v1.0.2-win-x64-portable.exe").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            parse_checksum_for_asset(text, "EnvStackUI-v1.0.2-win-x64-portable.zip").as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn swap_executable_files_replaces_and_keeps_backup() {
        let base = std::env::temp_dir().join(format!("envstack_swap_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let current = base.join("EnvStackUI.exe");
        let staged = base.join("envstack-ui.new.exe");
        fs::write(&current, b"old-binary").unwrap();
        fs::write(&staged, b"new-binary").unwrap();

        let backup = swap_executable_files(&current, &staged).unwrap();
        assert_eq!(backup, base.join("EnvStackUI.exe.old"));
        assert_eq!(fs::read(&backup).unwrap(), b"old-binary");
        assert_eq!(fs::read(&current).unwrap(), b"new-binary");
        assert!(!staged.exists());

        let _ = fs::remove_dir_all(&base);
    }
}
