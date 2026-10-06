use crate::theme::AppTheme;
use log::{info, warn};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SaveMode {
    Manual,       // 按下 Ctrl + S 手動保存
    AutoDebounce, // 打字停止 800ms 後自動防抖保存
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
            if let Err(e) = crate::document::atomic_write(&path, json.as_bytes()) {
                warn!("寫入偏好設定檔失敗: {}", e);
            } else {
                info!("已成功保存使用者偏好設定至 {:?}", path);
            }
        }
    }

    fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("finite configuration")
    }

    fn parse_json(s: &str) -> Result<Self, String> {
        let mut config: Self = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if !config.font_scale.is_finite() {
            return Err("字型比例必須為有限數值".to_string());
        }
        config.font_scale = config.font_scale.clamp(0.6_f32, 2.5_f32);
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
