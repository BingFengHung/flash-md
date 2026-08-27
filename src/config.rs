use crate::theme::AppTheme;
use log::{info, warn};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    Manual,       // 按下 Ctrl + S 手動保存
    AutoDebounce, // 打字停止 800ms 後自動防抖保存
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub theme: AppTheme,
    pub font_scale: f32,
    pub always_on_top: bool,
    pub save_mode: SaveMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: AppTheme::Light, // 預設使用使用者偏好的優雅亮色系！
            font_scale: 1.0_f32,
            always_on_top: false,
            save_mode: SaveMode::Manual,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        let dir = PathBuf::from(appdata).join("flash-md");
        let _ = fs::create_dir_all(&dir);
        Some(dir.join("config.json"))
    }

    pub fn load() -> Self {
        let config = Self::default();
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(loaded) = Self::parse_json(&content) {
                        info!("已載入使用者偏好設定: {:?}", loaded);
                        return loaded;
                    }
                }
            }
        }
        config
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            let json = self.to_json();
            if let Err(e) = fs::write(&path, json) {
                warn!("寫入偏好設定檔失敗: {}", e);
            } else {
                info!("已成功保存使用者偏好設定至 {:?}", path);
            }
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\n  \"theme\": \"{}\",\n  \"font_scale\": {:.2},\n  \"always_on_top\": {},\n  \"save_mode\": \"{}\"\n}}",
            match self.theme {
                AppTheme::Dark => "Dark",
                AppTheme::Light => "Light",
            },
            self.font_scale,
            self.always_on_top,
            match self.save_mode {
                SaveMode::Manual => "Manual",
                SaveMode::AutoDebounce => "AutoDebounce",
            }
        )
    }

    fn parse_json(s: &str) -> Result<Self, String> {
        let mut config = Self::default();
        for line in s.lines() {
            let trimmed = line.trim().trim_matches(',').trim();
            if trimmed.starts_with("\"theme\"") {
                if trimmed.contains("\"Dark\"") {
                    config.theme = AppTheme::Dark;
                } else if trimmed.contains("\"Light\"") {
                    config.theme = AppTheme::Light;
                }
            } else if trimmed.starts_with("\"font_scale\"") {
                if let Some(val_str) = trimmed.split(':').nth(1) {
                    if let Ok(val) = val_str.trim().parse::<f32>() {
                        config.font_scale = val.clamp(0.7_f32, 2.0_f32);
                    }
                }
            } else if trimmed.starts_with("\"always_on_top\"") {
                if trimmed.contains("true") {
                    config.always_on_top = true;
                } else if trimmed.contains("false") {
                    config.always_on_top = false;
                }
            } else if trimmed.starts_with("\"save_mode\"") {
                if trimmed.contains("\"AutoDebounce\"") {
                    config.save_mode = SaveMode::AutoDebounce;
                } else if trimmed.contains("\"Manual\"") {
                    config.save_mode = SaveMode::Manual;
                }
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_default_values() {
        let config = AppConfig::default();
        assert_eq!(config.theme, AppTheme::Light);
        assert_eq!(config.font_scale, 1.0_f32);
        assert!(!config.always_on_top);
        assert_eq!(config.save_mode, SaveMode::Manual);
    }

    #[test]
    fn test_app_config_json_roundtrip() {
        let config = AppConfig {
            theme: AppTheme::Dark,
            font_scale: 1.25_f32,
            always_on_top: true,
            save_mode: SaveMode::AutoDebounce,
        };

        let json = config.to_json();
        let loaded = AppConfig::parse_json(&json).expect("解析設定 JSON 失敗");

        assert_eq!(loaded.theme, AppTheme::Dark);
        assert!((loaded.font_scale - 1.25_f32).abs() < 0.01_f32);
        assert!(loaded.always_on_top);
        assert_eq!(loaded.save_mode, SaveMode::AutoDebounce);
    }

    #[test]
    fn test_app_config_corrupt_json_fallback() {
        let corrupt = "{ corrupted_data: null }";
        let loaded = AppConfig::parse_json(corrupt).unwrap_or_default();
        assert_eq!(loaded.theme, AppTheme::Light);
        assert_eq!(loaded.font_scale, 1.0_f32);
    }
}
