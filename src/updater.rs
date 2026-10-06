use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "BingFengHung/flash-md";

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub download_url: String,
    pub html_url: String,
    pub changelog: String,
    pub digest: Option<String>,
}

pub enum UpdateEvent {
    Checked(Result<Option<ReleaseInfo>, String>),
    Installed(Result<(), String>),
}

#[derive(Deserialize)]
struct ReleasePayload {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn asset_for_arch(arch: &str) -> Result<&'static str, String> {
    match arch {
        "x86_64" => Ok("flash-md-windows-x86_64.zip"),
        "aarch64" => Ok("flash-md-windows-aarch64.zip"),
        _ => Err(format!("不支援的更新架構：{}", arch)),
    }
}

pub fn is_newer_version(current: &str, remote: &str) -> bool {
    let parse = |s: &str| semver::Version::parse(s.trim_start_matches('v')).ok();
    match (parse(current), parse(remote)) {
        (Some(current), Some(remote)) => remote > current,
        _ => false,
    }
}

fn parse_release(json: &str, current: &str, arch: &str) -> Result<Option<ReleaseInfo>, String> {
    let release: ReleasePayload = serde_json::from_str(json.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("更新資訊格式無效：{}", e))?;
    semver::Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|e| format!("版本號無效：{}", e))?;
    if !is_newer_version(current, &release.tag_name) {
        return Ok(None);
    }
    let name = asset_for_arch(arch)?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("版本 {} 尚未提供 {}", release.tag_name, name))?;
    let prefix = format!("https://github.com/{}/releases/download/", GITHUB_REPO);
    if !asset.browser_download_url.starts_with(&prefix) {
        return Err("更新下載網址不屬於本專案".to_string());
    }
    Ok(Some(ReleaseInfo {
        tag_name: release.tag_name,
        download_url: asset.browser_download_url,
        html_url: release.html_url,
        changelog: release.body.unwrap_or_default(),
        digest: asset.digest,
    }))
}

fn powershell(script: &str) -> Result<Output, String> {
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = command
        .output()
        .map_err(|e| format!("無法執行更新程序：{}", e))?;
    if !output.status.success() {
        return Err(format!(
            "更新程序失敗（{}）：{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn check_latest_release() -> Result<Option<ReleaseInfo>, String> {
    let script = format!(
        r#"$ErrorActionPreference = 'Stop';
$ProgressPreference = 'SilentlyContinue';
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding;
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12;
$res = Invoke-RestMethod -Uri 'https://api.github.com/repos/{}/releases/latest' -Headers @{{ 'User-Agent' = 'flash-md-updater' }} -TimeoutSec 30;
$res | ConvertTo-Json -Depth 10 -Compress;"#,
        GITHUB_REPO
    );
    let output = powershell(&script)?;
    let json = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    parse_release(json.trim(), CURRENT_VERSION, env::consts::ARCH)
}

fn replace_executable(staged: &Path, target: &Path) -> Result<(), String> {
    replace_with(staged, target, |from, to| fs::rename(from, to))
}

fn replace_with(
    staged: &Path,
    target: &Path,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let mut name = target.as_os_str().to_os_string();
    name.push(".old");
    let backup = std::path::PathBuf::from(name);
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| format!("無法移除舊備份：{}", e))?;
    }
    rename(target, &backup).map_err(|e| format!("無法備份目前執行檔：{}", e))?;
    if let Err(error) = rename(staged, target) {
        return match rename(&backup, target) {
            Ok(()) => Err(format!("更新失敗，已還原原版本：{}", error)),
            Err(rollback) => Err(format!(
                "更新失敗：{}；無法自動還原：{}。備份位置：{}",
                error,
                rollback,
                backup.display()
            )),
        };
    }
    Ok(())
}

fn validate_executable(bytes: &[u8], arch: &str) -> Result<(), String> {
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err("下載內容不是 Windows 執行檔".to_string());
    }
    let offset = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
    let header = bytes
        .get(offset..offset.saturating_add(6))
        .ok_or("Windows 執行檔標頭不完整")?;
    if &header[..4] != b"PE\0\0" {
        return Err("Windows 執行檔標頭無效".to_string());
    }
    let expected = match arch {
        "x86_64" => 0x8664_u16,
        "aarch64" => 0xaa64_u16,
        _ => return Err("不支援的執行檔架構".to_string()),
    };
    if u16::from_le_bytes([header[4], header[5]]) != expected {
        return Err("下載執行檔的架構不符合目前版本".to_string());
    }
    Ok(())
}

pub fn perform_self_update(release: &ReleaseInfo) -> Result<(), String> {
    let target = env::current_exe().map_err(|e| e.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary =
        env::temp_dir().join(format!("flash-md-update-{}-{}", std::process::id(), nonce));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let staged = target.with_extension(format!("exe.update-{}-{}", std::process::id(), nonce));
    let result = (|| {
        let zip = temporary.join("release.zip");
        let extract = temporary.join("extract");
        let verify = match &release.digest {
            Some(digest) => {
                let hash = digest
                    .strip_prefix("sha256:")
                    .ok_or("不支援的更新校驗格式")?;
                if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("更新校驗碼格式無效".to_string());
                }
                format!("if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne {}) {{ throw 'SHA256 mismatch' }};", ps_literal(hash))
            }
            None => String::new(),
        };
        let script = format!(
            r#"$ErrorActionPreference = 'Stop';
$ProgressPreference = 'SilentlyContinue';
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12;
$zip = {};
Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile $zip -TimeoutSec 90;
{}
Expand-Archive -LiteralPath $zip -DestinationPath {} -Force;"#,
            ps_literal(&zip.to_string_lossy()),
            ps_literal(&release.download_url),
            verify,
            ps_literal(&extract.to_string_lossy())
        );
        powershell(&script)?;
        let executable = extract.join("flash-md.exe");
        let bytes = fs::read(&executable).map_err(|e| format!("找不到下載的執行檔：{}", e))?;
        validate_executable(&bytes, env::consts::ARCH)?;
        crate::document::atomic_write(&staged, &bytes)
            .map_err(|e| format!("無法準備更新檔：{}", e))?;
        replace_executable(&staged, &target)
    })();
    let _ = fs::remove_file(&staged);
    let _ = fs::remove_dir_all(&temporary);
    result
}

pub fn restart_with_new_version(args: &[String]) -> Result<(), String> {
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new(executable);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000200 | 0x00000008);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("無法啟動新版本：{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_follow_semver_and_invalid_input_is_not_newer() {
        assert!(is_newer_version("1.0.103", "v1.0.104"));
        assert!(is_newer_version("1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer_version("1.0.0", "broken"));
        assert!(!is_newer_version("1.0.104", "1.0.103"));
    }

    #[test]
    fn release_selection_respects_architecture_and_reports_missing_asset() {
        let json = r#"{"tag_name":"v1.1.0","html_url":"https://github.com/BingFengHung/flash-md/releases","body":"notes ||| with unicode 中文","assets":[{"name":"flash-md-windows-x86_64.zip","browser_download_url":"https://github.com/BingFengHung/flash-md/releases/download/v1.1.0/x64.zip"},{"name":"flash-md-windows-aarch64.zip","browser_download_url":"https://github.com/BingFengHung/flash-md/releases/download/v1.1.0/arm64.zip"}]}"#;
        assert!(parse_release(json, "1.0.0", "aarch64")
            .unwrap()
            .unwrap()
            .download_url
            .ends_with("arm64.zip"));
        assert!(parse_release(json, "1.0.0", "x86_64")
            .unwrap()
            .unwrap()
            .download_url
            .ends_with("x64.zip"));
        assert!(parse_release(json, "1.0.0", "unknown").is_err());
        assert!(parse_release("network error", "1.0.0", "x86_64").is_err());
        assert!(parse_release(json, "1.1.0", "x86_64").unwrap().is_none());
    }

    #[test]
    fn failed_install_rolls_back_the_original_executable() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("flash-md.exe");
        let staged = dir.path().join("new.exe");
        fs::write(&target, "original").unwrap();
        fs::write(&staged, "new").unwrap();
        let mut calls = 0;
        let result = replace_with(&staged, &target, |from, to| {
            calls += 1;
            if calls == 2 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "simulated install failure",
                ))
            } else {
                fs::rename(from, to)
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_to_string(&staged).unwrap(), "new");
        assert_eq!(calls, 3);
    }

    #[test]
    fn powershell_paths_preserve_backslashes_and_escape_quotes() {
        assert_eq!(
            ps_literal(r"C:\O'Brien\flash-md.exe"),
            r"'C:\O''Brien\flash-md.exe'"
        );
    }

    #[test]
    fn executable_validation_rejects_truncated_and_wrong_architecture() {
        assert!(validate_executable(b"not an exe", "x86_64").is_err());
        let mut exe = vec![0; 100];
        exe[..2].copy_from_slice(b"MZ");
        exe[60..64].copy_from_slice(&64_u32.to_le_bytes());
        exe[64..68].copy_from_slice(b"PE\0\0");
        exe[68..70].copy_from_slice(&0x8664_u16.to_le_bytes());
        assert!(validate_executable(&exe, "x86_64").is_ok());
        assert!(validate_executable(&exe, "aarch64").is_err());
    }
}
