use log::{error, info, warn};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "BingFengHung/flash-md";

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub version: String,
    pub download_url: String,
    pub html_url: String,
    pub changelog: String,
}

/// 向 GitHub API 檢查是否有新版本發布
pub fn check_latest_release() -> Option<ReleaseInfo> {
    info!("正在檢查 GitHub Releases 最新版本 (目前版本: v{})...", CURRENT_VERSION);

    #[cfg(windows)]
    {
        // 透過 PowerShell 查詢 GitHub API (避免額外引入肥重 HTTP/TLS crate)
        let ps_script = format!(
            r#"$ProgressPreference = 'SilentlyContinue';
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12;
$headers = @{{ 'User-Agent' = 'flash-md-updater' }};
try {{
    $res = Invoke-RestMethod -Uri 'https://api.github.com/repos/{}/releases/latest' -Headers $headers;
    $asset = $res.assets | Where-Object {{ $_.name -like '*windows-x86_64.zip' }} | Select-Object -First 1;
    $dUrl = if ($asset) {{ $asset.browser_download_url }} else {{ '' }};
    $body = ($res.body -replace "`r", "") -replace "`n", "<BR>";
    Write-Output "$($res.tag_name)|||$($dUrl)|||$($res.html_url)|||$body"
}} catch {{
    Write-Output "ERROR: $($_.Exception.Message)"
}}"#,
            GITHUB_REPO
        );

        let output = Command::new("powershell")
            .args(&["-NoProfile", "-Command", &ps_script])
            .output();

        if let Ok(out) = output {
            let res_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if res_str.starts_with("ERROR:") || res_str.is_empty() {
                warn!("檢查更新失敗: {}", res_str);
                return None;
            }

            let parts: Vec<&str> = res_str.split("|||").collect();
            if parts.len() >= 4 {
                let tag_name = parts[0].trim().to_string();
                let download_url = parts[1].trim().to_string();
                let html_url = parts[2].trim().to_string();
                let changelog = parts[3].replace("<BR>", "\n").trim().to_string();

                let remote_ver = tag_name.trim_start_matches('v');
                if is_newer_version(CURRENT_VERSION, remote_ver) {
                    info!("🎉 發現新版本: {} (當前版本: v{})", tag_name, CURRENT_VERSION);
                    return Some(ReleaseInfo {
                        tag_name,
                        version: remote_ver.to_string(),
                        download_url,
                        html_url,
                        changelog,
                    });
                } else {
                    info!("✅ 目前已是最新版本 (v{})", CURRENT_VERSION);
                }
            }
        }
    }

    None
}

/// 比較兩個語意化版本號，若 remote > current 則傳回 true
pub fn is_newer_version(current: &str, remote: &str) -> bool {
    let parse_ver = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };

    let cur_parts = parse_ver(current);
    let rem_parts = parse_ver(remote);

    for (c, r) in cur_parts.iter().zip(rem_parts.iter()) {
        if r > c {
            return true;
        } else if r < c {
            return false;
        }
    }

    rem_parts.len() > cur_parts.len()
}

/// 自動下載最新 Release 並覆蓋更新當前執行檔 (Windows Hot-Swap)
pub fn perform_self_update(release: &ReleaseInfo) -> Result<(), String> {
    if release.download_url.is_empty() {
        return Err("未找到 Windows 執行檔下載連結".to_string());
    }

    let current_exe = env::current_exe().map_err(|e| format!("無法取得當前執行檔路徑: {}", e))?;
    let exe_dir = current_exe.parent().ok_or("無法取得執行檔目錄")?;

    info!("開始下載並自動更新至 {}...", release.tag_name);

    #[cfg(windows)]
    {
        let ps_script = format!(
            r#"$ProgressPreference = 'SilentlyContinue';
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12;
$zipPath = Join-Path $env:TEMP 'flash-md-update.zip';
$extractPath = Join-Path $env:TEMP 'flash-md-update-extract';
if (Test-Path $extractPath) {{ Remove-Item -Recurse -Force $extractPath }};
New-Item -ItemType Directory -Path $extractPath | Out-Null;

Write-Host "正在下載最新版本...";
Invoke-WebRequest -Uri '{}' -OutFile $zipPath;

Write-Host "正在解壓縮...";
Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force;

$newExe = Get-ChildItem -Path $extractPath -Filter "flash-md.exe" -Recurse | Select-Object -First 1;
if (-not $newExe) {{
    throw "解壓縮後未找到 flash-md.exe";
}}

$targetExe = '{}';
$backupExe = "$targetExe.old";
if (Test-Path $backupExe) {{ Remove-Item -Force $backupExe }};

Write-Host "替換執行檔中...";
Move-Item -Path $targetExe -Destination $backupExe -Force;
Copy-Item -Path $newExe.FullName -Destination $targetExe -Force;
Remove-Item -Force $zipPath;
Remove-Item -Recurse -Force $extractPath;

Write-Host "SUCCESS";
"#,
            release.download_url,
            current_exe.to_string_lossy().replace('\\', "\\\\")
        );

        let output = Command::new("powershell")
            .args(&["-NoProfile", "-Command", &ps_script])
            .output()
            .map_err(|e| format!("執行更新腳本失敗: {}", e))?;

        let out_str = String::from_utf8_lossy(&output.stdout);
        if out_str.contains("SUCCESS") {
            info!("✅ 自動更新成功！已升級至 {}", release.tag_name);
            Ok(())
        } else {
            let err_str = String::from_utf8_lossy(&output.stderr);
            Err(format!("更新過程發生錯誤: {}\n{}", out_str.trim(), err_str.trim()))
        }
    }

    #[cfg(not(windows))]
    {
        Err("目前僅支援 Windows 自動更新".to_string())
    }
}
