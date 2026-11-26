//! GitHub Releases online update checker, SHA256 verifier, and atomic Windows portable EXE swapper.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;

pub const DEFAULT_GITHUB_REPO: &str = "NGLS-ls/LumenHID";
pub const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

static CRYPTO_INIT: Once = Once::new();
static UPDATE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct UpdateBusyGuard;

impl Drop for UpdateBusyGuard {
    fn drop(&mut self) {
        UPDATE_IN_PROGRESS.store(false, Ordering::Release);
    }
}

pub type ProgressCallback = Arc<dyn Fn(UpdateProgress) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UpdaterConfigFile {
    repo: String,
    #[serde(default)]
    github_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdaterSettingsView {
    pub current_version: String,
    pub repo: String,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub tag_name: Option<String>,
    pub has_update: bool,
    pub repo: String,
    pub has_token: bool,
    pub release_title: Option<String>,
    pub release_notes: Option<String>,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
    pub download_url: Option<String>,
    pub api_asset_url: Option<String>,
    pub checksum_url: Option<String>,
    pub checksum_api_url: Option<String>,
    pub auth_required: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateApplyResult {
    pub will_restart: bool,
    pub staged_path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubReleaseAsset {
    pub name: String,
    pub url: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<GitHubReleaseAsset>,
}

/// Validates a `owner/repo` slug strictly to avoid URL injection or path traversal.
pub fn validate_repo_slug(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 2 {
        return Err("仓库格式必须为 owner/repo，例如 NGLS-ls/LumenHID".to_string());
    }
    for part in &parts {
        if part.is_empty()
            || *part == "."
            || *part == ".."
            || part.contains("..")
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err("仓库名称包含非法字符，仅支持英文字母、数字、-、_、.".to_string());
        }
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}

/// Parses `v1.2.3`, `1.2`, or `v1.2.3-beta.1+build` into `((major, minor, patch), prerelease)`.
pub fn parse_semver(raw: &str) -> Option<((u64, u64, u64), Option<String>)> {
    let stripped = raw.trim().trim_start_matches(['v', 'V']);
    if stripped.is_empty() {
        return None;
    }
    let without_build = stripped.split('+').next().unwrap_or(stripped);
    let (core, pre) = match without_build.split_once('-') {
        Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
        Some((c, _)) => (c, None),
        None => (without_build, None),
    };
    let nums: Vec<&str> = core.split('.').collect();
    if !(2..=3).contains(&nums.len()) {
        return None;
    }
    let major = nums[0].parse::<u64>().ok()?;
    let minor = nums[1].parse::<u64>().ok()?;
    let patch = if nums.len() == 3 {
        nums[2].parse::<u64>().ok()?
    } else {
        0
    };
    Some(((major, minor, patch), pre))
}

/// Returns `true` only if `latest` is strictly newer than `current`.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let Some((cur_ver, cur_pre)) = parse_semver(current) else {
        return false;
    };
    let Some((lat_ver, lat_pre)) = parse_semver(latest) else {
        return false;
    };
    if lat_ver != cur_ver {
        return lat_ver > cur_ver;
    }
    matches!((cur_pre.is_some(), lat_pre.is_none()), (true, true))
}

/// Selects the portable EXE asset (`LumenHID.exe` preferred) and optional `SHA256SUMS.txt`.
pub fn select_release_assets(
    assets: &[GitHubReleaseAsset],
) -> (Option<&GitHubReleaseAsset>, Option<&GitHubReleaseAsset>) {
    let exe = assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("lumenhid.exe"))
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
        });
    let sha = assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("sha256sums.txt"));
    (exe, sha)
}

/// Extracts lowercase 64-hex SHA256 digest for `asset_name` from `SHA256SUMS.txt`.
pub fn parse_sha256_for_asset(sums_text: &str, asset_name: &str) -> Option<String> {
    let target_lower = asset_name.trim().to_ascii_lowercase();
    for raw_line in sums_text.trim_start_matches('\u{feff}').lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(file_part) = parts.next() else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let clean_name = file_part
            .trim_start_matches('*')
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_part)
            .to_ascii_lowercase();
        if clean_name == target_lower {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

/// Streams a local file in 64 KiB chunks and returns its lowercase SHA256 hex digest.
pub fn compute_sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("打开待校验文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取文件计算 SHA256 失败: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    Ok(out)
}

fn updater_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("updater.json")
}

fn load_effective_config(data_dir: &Path) -> (String, Option<String>) {
    let cfg_path = updater_config_path(data_dir);
    let parsed = fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<UpdaterConfigFile>(&s).ok())
        .unwrap_or_default();

    let repo = validate_repo_slug(&parsed.repo)
        .unwrap_or_else(|_| DEFAULT_GITHUB_REPO.to_string());

    let saved_token = parsed
        .github_token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let token = saved_token.or_else(|| {
        std::env::var("LUMENHID_GITHUB_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    });

    (repo, token)
}

pub fn get_updater_settings(data_dir: &Path) -> UpdaterSettingsView {
    let (repo, token) = load_effective_config(data_dir);
    UpdaterSettingsView {
        current_version: CURRENT_APP_VERSION.to_string(),
        repo,
        has_token: token.is_some(),
    }
}

pub fn save_updater_settings(
    data_dir: &Path,
    repo: &str,
    github_token: Option<String>,
    clear_token: bool,
) -> Result<UpdaterSettingsView, String> {
    let valid_repo = validate_repo_slug(repo)?;
    let cfg_path = updater_config_path(data_dir);
    let existing_token = fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<UpdaterConfigFile>(&s).ok())
        .and_then(|c| c.github_token)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let next_token = if clear_token {
        None
    } else if let Some(raw) = github_token {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            existing_token
        } else {
            Some(trimmed)
        }
    } else {
        existing_token
    };

    let file_cfg = UpdaterConfigFile {
        repo: valid_repo,
        github_token: next_token,
    };
    let json = serde_json::to_vec_pretty(&file_cfg).map_err(|e| e.to_string())?;
    let tmp_path = data_dir.join("updater.json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("写入更新配置失败: {e}"))?;
    if cfg_path.exists() {
        let _ = fs::remove_file(&cfg_path);
    }
    fs::rename(&tmp_path, &cfg_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("保存更新配置失败: {e}")
    })?;

    Ok(get_updater_settings(data_dir))
}

fn ensure_crypto_provider() {
    CRYPTO_INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn build_http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .user_agent(format!("LumenHID/{CURRENT_APP_VERSION}"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .map_err(|e| format!("初始化 HTTPS 客户端失败: {e}"))
}

fn apply_github_api_headers(
    req: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut r = req
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = token {
        r = r.bearer_auth(t);
    }
    r
}

pub async fn check_for_update(data_dir: &Path) -> Result<UpdateCheckResult, String> {
    let (repo, token) = load_effective_config(data_dir);
    let client = build_http_client(Duration::from_secs(15))?;

    let latest_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = apply_github_api_headers(client.get(&latest_url), token.as_deref())
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 更新检查失败: {e}"))?;

    let status = resp.status();
    let release_opt: Option<GitHubRelease> = if status.is_success() {
        let rel = resp
            .json::<GitHubRelease>()
            .await
            .map_err(|e| format!("解析 GitHub Release JSON 失败: {e}"))?;
        if rel.draft {
            None
        } else {
            Some(rel)
        }
    } else if status == reqwest::StatusCode::NOT_FOUND {
        let list_url = format!("https://api.github.com/repos/{repo}/releases?per_page=5");
        let list_resp = apply_github_api_headers(client.get(&list_url), token.as_deref())
            .send()
            .await
            .map_err(|e| format!("回退查询 GitHub Releases 列表失败: {e}"))?;
        let list_status = list_resp.status();
        if list_status.is_success() {
            let list = list_resp
                .json::<Vec<GitHubRelease>>()
                .await
                .map_err(|e| format!("解析 Releases 列表失败: {e}"))?;
            list.into_iter().find(|r| !r.draft)
        } else if list_status == reqwest::StatusCode::UNAUTHORIZED
            || list_status == reqwest::StatusCode::FORBIDDEN
            || (list_status == reqwest::StatusCode::NOT_FOUND && token.is_none())
        {
            return Ok(UpdateCheckResult {
                current_version: CURRENT_APP_VERSION.to_string(),
                latest_version: None,
                tag_name: None,
                has_update: false,
                repo,
                has_token: token.is_some(),
                release_title: None,
                release_notes: None,
                release_url: None,
                published_at: None,
                asset_name: None,
                asset_size: None,
                download_url: None,
                api_asset_url: None,
                checksum_url: None,
                checksum_api_url: None,
                auth_required: true,
                detail: "未检测到公开 Release；若仓库为私有，请在更新设置中填入 GitHub Token，或先推送 v* 标签发布 Release"
                    .to_string(),
            });
        } else {
            return Err(format!("GitHub Releases 接口返回 HTTP {list_status}"));
        }
    } else if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
    {
        return Ok(UpdateCheckResult {
            current_version: CURRENT_APP_VERSION.to_string(),
            latest_version: None,
            tag_name: None,
            has_update: false,
            repo,
            has_token: token.is_some(),
            release_title: None,
            release_notes: None,
            release_url: None,
            published_at: None,
            asset_name: None,
            asset_size: None,
            download_url: None,
            api_asset_url: None,
            checksum_url: None,
            checksum_api_url: None,
            auth_required: true,
            detail: "GitHub 接口拒绝访问或需要鉴权（401/403），请在更新设置中配置有效的 GitHub Token"
                .to_string(),
        });
    } else {
        return Err(format!("GitHub Release 查询失败，HTTP {status}"));
    };

    let Some(release) = release_opt else {
        return Ok(UpdateCheckResult {
            current_version: CURRENT_APP_VERSION.to_string(),
            latest_version: None,
            tag_name: None,
            has_update: false,
            repo,
            has_token: token.is_some(),
            release_title: None,
            release_notes: None,
            release_url: None,
            published_at: None,
            asset_name: None,
            asset_size: None,
            download_url: None,
            api_asset_url: None,
            checksum_url: None,
            checksum_api_url: None,
            auth_required: token.is_none(),
            detail: "仓库暂无已发布的正式 Release（请先在 GitHub 推送 v* 标签或创建 Release）"
                .to_string(),
        });
    };

    let latest_clean = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    let has_update = is_newer_version(CURRENT_APP_VERSION, &release.tag_name);
    let (exe_asset, sha_asset) = select_release_assets(&release.assets);

    let detail = if !has_update {
        format!("当前已是最新版本 v{CURRENT_APP_VERSION}（远端最新 {}）", release.tag_name)
    } else if let Some(exe) = exe_asset {
        format!(
            "发现新版本 {}（{}，{:.1} MB），点击即可拉取并自动升级",
            release.tag_name,
            exe.name,
            exe.size as f64 / (1024.0 * 1024.0)
        )
    } else {
        format!(
            "发现新版本 {}，但 Release 中暂未附带 .exe 安装包（可能 CI 仍在构建中）",
            release.tag_name
        )
    };

    Ok(UpdateCheckResult {
        current_version: CURRENT_APP_VERSION.to_string(),
        latest_version: Some(latest_clean),
        tag_name: Some(release.tag_name),
        has_update,
        repo,
        has_token: token.is_some(),
        release_title: release.name,
        release_notes: release.body,
        release_url: Some(release.html_url),
        published_at: release.published_at,
        asset_name: exe_asset.map(|a| a.name.clone()),
        asset_size: exe_asset.map(|a| a.size),
        download_url: exe_asset.map(|a| a.browser_download_url.clone()),
        api_asset_url: exe_asset.map(|a| a.url.clone()),
        checksum_url: sha_asset.map(|a| a.browser_download_url.clone()),
        checksum_api_url: sha_asset.map(|a| a.url.clone()),
        auth_required: false,
        detail,
    })
}

fn sanitize_version_for_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "latest".to_string()
    } else {
        cleaned
    }
}

fn emit_progress(cb: &Option<ProgressCallback>, progress: UpdateProgress) {
    if let Some(f) = cb {
        f(progress);
    }
}

fn escape_ps_single_quoted(raw: &str) -> String {
    raw.replace('\'', "''")
}

/// Builds the detached Windows PowerShell swapper script that waits for `old_pid`, backs up
/// `target_exe` to `.bak`, replaces it with `staged_exe`, rolls back on failure, and relaunches.
pub fn build_windows_swap_script(old_pid: u32, target_exe: &Path, staged_exe: &Path) -> String {
    let target = escape_ps_single_quoted(&target_exe.to_string_lossy());
    let staged = escape_ps_single_quoted(&staged_exe.to_string_lossy());
    let work_dir = target_exe
        .parent()
        .map(|p| escape_ps_single_quoted(&p.to_string_lossy()))
        .unwrap_or_else(|| ".".to_string());

    format!(
        r#"$ErrorActionPreference = 'Stop'
$OldPid = {old_pid}
$TargetExe = '{target}'
$StagedExe = '{staged}'
$BackupExe = '{target}.bak'
$WorkDir = '{work_dir}'

for ($i = 0; $i -lt 40; $i++) {{
    $proc = Get-Process -Id $OldPid -ErrorAction SilentlyContinue
    if (-not $proc) {{ break }}
    Start-Sleep -Milliseconds 500
}}

if (Test-Path -LiteralPath $TargetExe) {{
    Copy-Item -LiteralPath $TargetExe -Destination $BackupExe -Force
}}

$replaced = $false
for ($attempt = 0; $attempt -lt 15; $attempt++) {{
    try {{
        Copy-Item -LiteralPath $StagedExe -Destination $TargetExe -Force
        $replaced = $true
        break
    }} catch {{
        Start-Sleep -Milliseconds 500
    }}
}}

if (-not $replaced) {{
    if (Test-Path -LiteralPath $BackupExe) {{
        try {{ Copy-Item -LiteralPath $BackupExe -Destination $TargetExe -Force }} catch {{}}
    }}
    exit 1
}}

try {{ Remove-Item -LiteralPath $StagedExe -Force -ErrorAction SilentlyContinue }} catch {{}}
Start-Process -FilePath $TargetExe -WorkingDirectory $WorkDir
try {{ Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue }} catch {{}}
"#
    )
}

async fn download_text_asset(
    client: &reqwest::Client,
    token: Option<&str>,
    api_url: Option<&str>,
    browser_url: &str,
) -> Result<String, String> {
    let req = match (token, api_url) {
        (Some(t), Some(api)) => client
            .get(api)
            .bearer_auth(t)
            .header("Accept", "application/octet-stream")
            .header("X-GitHub-Api-Version", "2022-11-28"),
        _ => client.get(browser_url),
    };
    let resp = req
        .send()
        .await
        .map_err(|e| format!("下载 SHA256SUMS.txt 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "下载 SHA256SUMS.txt 返回 HTTP {}",
            resp.status()
        ));
    }
    resp.text()
        .await
        .map_err(|e| format!("读取 SHA256SUMS.txt 文本失败: {e}"))
}

pub async fn download_and_apply_update(
    data_dir: &Path,
    update: &UpdateCheckResult,
    progress_cb: Option<ProgressCallback>,
) -> Result<UpdateApplyResult, String> {
    if UPDATE_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("已有在线更新任务正在执行，请等待当前任务完成".to_string());
    }
    let _busy_guard = UpdateBusyGuard;

    if !update.has_update {
        return Err("当前没有可升级的新版本".to_string());
    }
    let download_url = update
        .download_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "该 Release 尚未附带可下载的 Windows EXE 文件".to_string())?;
    let asset_name = update
        .asset_name
        .as_deref()
        .unwrap_or("LumenHID.exe")
        .to_string();
    let version_tag = sanitize_version_for_filename(
        update
            .latest_version
            .as_deref()
            .or(update.tag_name.as_deref())
            .unwrap_or("latest"),
    );

    let (_, token) = load_effective_config(data_dir);
    let client = build_http_client(Duration::from_secs(300))?;

    let updates_dir = data_dir.join("updates");
    fs::create_dir_all(&updates_dir).map_err(|e| format!("创建更新暂存目录失败: {e}"))?;
    let part_path = updates_dir.join(format!("LumenHID-{version_tag}.exe.part"));
    let staged_path = updates_dir.join(format!("LumenHID-{version_tag}.exe"));
    if part_path.exists() {
        let _ = fs::remove_file(&part_path);
    }

    emit_progress(
        &progress_cb,
        UpdateProgress {
            stage: "downloading".to_string(),
            downloaded_bytes: 0,
            total_bytes: update.asset_size,
            percent: 0,
            message: format!("正在从 GitHub 拉取 {asset_name}..."),
        },
    );

    let req = match (token.as_deref(), update.api_asset_url.as_deref()) {
        (Some(t), Some(api)) => client
            .get(api)
            .bearer_auth(t)
            .header("Accept", "application/octet-stream")
            .header("X-GitHub-Api-Version", "2022-11-28"),
        _ => client.get(download_url),
    };

    let mut resp = req
        .send()
        .await
        .map_err(|e| format!("连接 GitHub 下载资产失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载新版本返回 HTTP {}", resp.status()));
    }

    let total_bytes = resp.content_length().or(update.asset_size);
    let mut part_file =
        fs::File::create(&part_path).map_err(|e| format!("创建下载缓存文件失败: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_reported_bytes: u64 = 0;
    let mut last_reported_pct: u8 = 0;

    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                part_file
                    .write_all(&chunk)
                    .map_err(|e| format!("写入更新包缓存失败: {e}"))?;
                downloaded = downloaded.saturating_add(chunk.len() as u64);

                let pct = match total_bytes {
                    Some(total) if total > 0 => ((downloaded.saturating_mul(100)) / total).min(99) as u8,
                    _ => 0,
                };
                if pct >= last_reported_pct.saturating_add(2)
                    || downloaded.saturating_sub(last_reported_bytes) >= 262_144
                {
                    last_reported_pct = pct;
                    last_reported_bytes = downloaded;
                    emit_progress(
                        &progress_cb,
                        UpdateProgress {
                            stage: "downloading".to_string(),
                            downloaded_bytes: downloaded,
                            total_bytes,
                            percent: pct,
                            message: format!(
                                "正在下载 {asset_name}（{:.1} MB）...",
                                downloaded as f64 / (1024.0 * 1024.0)
                            ),
                        },
                    );
                }
            }
            Ok(None) => break,
            Err(e) => {
                drop(part_file);
                let _ = fs::remove_file(&part_path);
                return Err(format!("下载流中断: {e}"));
            }
        }
    }
    part_file
        .flush()
        .map_err(|e| format!("刷新更新包写入缓冲失败: {e}"))?;
    drop(part_file);

    emit_progress(
        &progress_cb,
        UpdateProgress {
            stage: "verifying".to_string(),
            downloaded_bytes: downloaded,
            total_bytes,
            percent: 100,
            message: "下载完成，正在校验 PE 文件头与 SHA256 指纹...".to_string(),
        },
    );

    let meta = fs::metadata(&part_path).map_err(|e| format!("读取已下载文件失败: {e}"))?;
    if meta.len() < 1_000_000 {
        let _ = fs::remove_file(&part_path);
        return Err(format!(
            "下载产物大小异常（{} 字节，低于 1MB 安全下限），已中止升级",
            meta.len()
        ));
    }

    {
        let mut f = fs::File::open(&part_path).map_err(|e| format!("校验 PE 头失败: {e}"))?;
        let mut magic = [0u8; 2];
        f.read_exact(&mut magic)
            .map_err(|e| format!("读取 PE 魔数失败: {e}"))?;
        if &magic != b"MZ" {
            let _ = fs::remove_file(&part_path);
            return Err("安全护栏拦截：下载产物不是有效的 Windows PE 可执行文件（缺少 MZ 头）".to_string());
        }
    }

    if let Some(checksum_browser_url) = update.checksum_url.as_deref() {
        let sums_text = download_text_asset(
            &client,
            token.as_deref(),
            update.checksum_api_url.as_deref(),
            checksum_browser_url,
        )
        .await?;
        if let Some(expected_hash) = parse_sha256_for_asset(&sums_text, &asset_name) {
            let actual_hash = compute_sha256_hex(&part_path)?;
            if actual_hash != expected_hash {
                let _ = fs::remove_file(&part_path);
                return Err(format!(
                    "SHA256 校验不匹配！预期 {expected_hash}，实际 {actual_hash}，已删除损坏安装包"
                ));
            }
        }
    }

    if staged_path.exists() {
        let _ = fs::remove_file(&staged_path);
    }
    fs::rename(&part_path, &staged_path).map_err(|e| {
        let _ = fs::remove_file(&part_path);
        format!("移动暂存安装包失败: {e}")
    })?;

    let current_exe = std::env::current_exe().map_err(|e| format!("定位当前程序路径失败: {e}"))?;
    let exe_lower = current_exe.to_string_lossy().to_ascii_lowercase();
    if exe_lower.contains("\\target\\debug\\") || exe_lower.contains("\\target\\release\\") {
        let staged_str = staged_path.to_string_lossy().to_string();
        emit_progress(
            &progress_cb,
            UpdateProgress {
                stage: "ready_dev".to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                percent: 100,
                message: format!("开发模式校验完成，新版已缓存至 {staged_str}"),
            },
        );
        return Ok(UpdateApplyResult {
            will_restart: false,
            staged_path: staged_str.clone(),
            detail: format!(
                "开发模式下已完成下载与 SHA256 校验，新版本保存在 {staged_str}（便携版 EXE 运行时将自动替换并重启）"
            ),
        });
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        emit_progress(
            &progress_cb,
            UpdateProgress {
                stage: "applying".to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                percent: 100,
                message: "正在创建原程序 .bak 备份并准备自动替换重启...".to_string(),
            },
        );

        let script_path = updates_dir.join("apply_update.ps1");
        let script_body =
            build_windows_swap_script(std::process::id(), &current_exe, &staged_path);
        let mut bom_bytes = vec![0xEF, 0xBB, 0xBF];
        bom_bytes.extend_from_slice(script_body.as_bytes());
        fs::write(&script_path, bom_bytes)
            .map_err(|e| format!("写入自动更新替换脚本失败: {e}"))?;

        crate::keyboard_engine::shutdown();

        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script_path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("拉起后台替换进程失败: {e}"))?;

        let staged_str = staged_path.to_string_lossy().to_string();
        Ok(UpdateApplyResult {
            will_restart: true,
            staged_path: staged_str,
            detail: "新版本校验通过，正在自动备份旧版并重启 LumenHID...".to_string(),
        })
    }

    #[cfg(not(windows))]
    {
        let staged_str = staged_path.to_string_lossy().to_string();
        Ok(UpdateApplyResult {
            will_restart: false,
            staged_path: staged_str.clone(),
            detail: format!("非 Windows 平台已完成下载校验，新版路径: {staged_str}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares_semver_versions() {
        assert_eq!(parse_semver("1.0.0"), Some(((1, 0, 0), None)));
        assert_eq!(parse_semver("v1.2.3"), Some(((1, 2, 3), None)));
        assert_eq!(parse_semver("V2.4"), Some(((2, 4, 0), None)));
        assert_eq!(
            parse_semver("v1.0.0-beta.1+build5"),
            Some(((1, 0, 0), Some("beta.1".to_string())))
        );
        assert_eq!(parse_semver("invalid"), None);

        assert!(is_newer_version("1.0.0", "v1.0.1"));
        assert!(is_newer_version("1.0.0", "v1.1.0"));
        assert!(is_newer_version("1.0.0", "2.0.0"));
        assert!(is_newer_version("1.0.0-beta.1", "v1.0.0"));
        assert!(!is_newer_version("1.0.0", "v1.0.0"));
        assert!(!is_newer_version("1.2.0", "v1.1.9"));
        assert!(!is_newer_version("1.0.0", "v1.0.0-rc.1"));
    }

    #[test]
    fn parses_sha256sums_with_bom_crlf_and_binary_prefix() {
        let sample = "\u{feff}e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  *LumenHID-Portable-win-x64.zip\r\nA1B2C3D4E5F60718293A4B5C6D7E8F90123456789ABCDEF0123456789ABCDEF0  LumenHID.exe\r\n";
        let got = parse_sha256_for_asset(sample, "lumenhid.exe").unwrap();
        assert_eq!(
            got,
            "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0"
        );

        let bad_hash = "1234  LumenHID.exe\n";
        assert!(parse_sha256_for_asset(bad_hash, "LumenHID.exe").is_none());
    }

    #[test]
    fn selects_portable_exe_and_sha256_from_ci_assets() {
        let assets = vec![
            GitHubReleaseAsset {
                name: "LumenHID-Portable-win-x64.zip".to_string(),
                url: "https://api.github.com/repos/NGLS-ls/LumenHID/releases/assets/1".to_string(),
                browser_download_url: "https://github.com/1".to_string(),
                size: 12_000_000,
            },
            GitHubReleaseAsset {
                name: "LumenHID.exe".to_string(),
                url: "https://api.github.com/repos/NGLS-ls/LumenHID/releases/assets/2".to_string(),
                browser_download_url: "https://github.com/2".to_string(),
                size: 14_000_000,
            },
            GitHubReleaseAsset {
                name: "SHA256SUMS.txt".to_string(),
                url: "https://api.github.com/repos/NGLS-ls/LumenHID/releases/assets/3".to_string(),
                browser_download_url: "https://github.com/3".to_string(),
                size: 128,
            },
        ];

        let (exe, sha) = select_release_assets(&assets);
        assert_eq!(exe.unwrap().name, "LumenHID.exe");
        assert_eq!(sha.unwrap().name, "SHA256SUMS.txt");
    }

    #[test]
    fn validates_github_repo_slug_strictly() {
        assert_eq!(
            validate_repo_slug(" NGLS-ls/LumenHID/ ").unwrap(),
            "NGLS-ls/LumenHID"
        );
        assert!(validate_repo_slug("https://github.com/NGLS-ls/LumenHID").is_err());
        assert!(validate_repo_slug("NGLS-ls/../LumenHID").is_err());
        assert!(validate_repo_slug("LumenHID").is_err());
        assert!(validate_repo_slug("owner/repo with space").is_err());
    }

    #[test]
    fn builds_swap_script_with_escaped_paths_and_backup_rollback() {
        let script = build_windows_swap_script(
            4242,
            Path::new(r"D:\dev\O'Reilly 测试\LumenHID.exe"),
            Path::new(r"D:\dev\O'Reilly 测试\LumenHID.data\updates\LumenHID-1.1.0.exe"),
        );
        assert!(script.contains("$OldPid = 4242"));
        assert!(script.contains(r"D:\dev\O''Reilly 测试\LumenHID.exe"));
        assert!(script.contains("$BackupExe = "));
        assert!(script.contains("Copy-Item -LiteralPath $BackupExe -Destination $TargetExe -Force"));
        assert!(script.contains("Start-Process -FilePath $TargetExe -WorkingDirectory $WorkDir"));
    }

    #[test]
    fn saves_and_masks_updater_settings_without_leaking_token() {
        let tmp = std::env::temp_dir().join(format!("lumenhid_updater_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let view = save_updater_settings(
            &tmp,
            "NGLS-ls/LumenHID",
            Some("ghp_secret_example_token".to_string()),
            false,
        )
        .unwrap();
        assert_eq!(view.repo, "NGLS-ls/LumenHID");
        assert!(view.has_token);

        let cleared = save_updater_settings(&tmp, "NGLS-ls/LumenHID", None, true).unwrap();
        assert_eq!(cleared.repo, "NGLS-ls/LumenHID");

        let _ = fs::remove_dir_all(&tmp);
    }
}
