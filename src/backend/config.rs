use crate::constants;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct BeepConfig {
    pub freq: u32,
    pub duration: u32,
    pub count: u32,
}

impl Default for BeepConfig {
    fn default() -> Self {
        BeepConfig {
            freq: 650,
            duration: 180,
            count: 1,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AfkConfig {
    pub enabled: bool,
    pub timeout: u32,
}

impl Default for AfkConfig {
    fn default() -> Self {
        AfkConfig {
            enabled: false,
            timeout: 60,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct SoundConfig {
    pub file: String,
    pub volume: u32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        SoundConfig {
            file: String::new(),
            volume: 50,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub device_id: Option<String>,
    pub sync_ids: Vec<String>,
    pub beep_enabled: bool,
    pub audio_mode: String, // "beep" or "custom"

    #[serde(rename = "beep_config")]
    pub beep_mode_configs: std::collections::HashMap<String, BeepConfig>,
    #[serde(rename = "sound_config")]
    pub sound_mode_configs: std::collections::HashMap<String, SoundConfig>,

    pub hotkey: std::collections::HashMap<String, serde_json::Value>,
    pub hotkey_mode: String, // "toggle" or "separate"

    pub afk: AfkConfig,

    pub persistent_overlay: OverlayConfig,
    pub osd: OsdConfig,
}

fn default_hotkey_mode() -> String {
    "toggle".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct OverlayConfig {
    pub enabled: bool,
    pub show_vu: bool,
    pub opacity: u8,
    pub x: i32,
    pub y: i32,
    pub position_mode: String,
    pub locked: bool,
    pub sensitivity: u32,
    pub device_id: Option<String>,
    pub scale: u32,
    pub theme: String,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        OverlayConfig {
            enabled: false,
            show_vu: false,
            opacity: 80,
            x: 100,
            y: 100,
            position_mode: "Custom".to_string(),
            locked: false,
            sensitivity: 5,
            device_id: None,
            scale: 100,
            theme: "Auto".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct OsdConfig {
    pub enabled: bool,
    pub duration: u32,
    pub position: String,
    pub size: u32,
    pub opacity: u8,
}

impl Default for OsdConfig {
    fn default() -> Self {
        OsdConfig {
            enabled: false,
            duration: 1500,
            position: "Bottom".to_string(),
            size: 150,
            opacity: 80,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut beep_mode_configs = std::collections::HashMap::new();
        beep_mode_configs.insert(
            "mute".to_string(),
            BeepConfig {
                freq: 650,
                duration: 180,
                count: 2,
            },
        );
        beep_mode_configs.insert(
            "unmute".to_string(),
            BeepConfig {
                freq: 700,
                duration: 200,
                count: 1,
            },
        );

        let mut sound_mode_configs = std::collections::HashMap::new();
        sound_mode_configs.insert(
            "mute".to_string(),
            SoundConfig {
                file: "mute.wav".to_string(),
                volume: 50,
            },
        );
        sound_mode_configs.insert(
            "unmute".to_string(),
            SoundConfig {
                file: "unmute.wav".to_string(),
                volume: 50,
            },
        );

        let mut hotkey = std::collections::HashMap::new();
        hotkey.insert(
            "toggle".to_string(),
            serde_json::json!({ "vk": 0xB3, "name": "Media Play/Pause" }),
        );
        hotkey.insert(
            "mute".to_string(),
            serde_json::json!({ "vk": 0, "name": "None" }),
        );
        hotkey.insert(
            "unmute".to_string(),
            serde_json::json!({ "vk": 0, "name": "None" }),
        );

        Self {
            device_id: None,
            sync_ids: vec![],
            beep_enabled: true,
            audio_mode: "beep".to_string(),
            beep_mode_configs,
            sound_mode_configs,
            hotkey,
            hotkey_mode: default_hotkey_mode(),
            afk: AfkConfig::default(),
            persistent_overlay: OverlayConfig::default(),
            osd: OsdConfig::default(),
        }
    }
}

impl AppConfig {
    fn get_config_path() -> Option<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "MicMute") {
            let data_dir = proj_dirs.data_local_dir();
            fs::create_dir_all(data_dir).ok()?;
            Some(data_dir.join("mic_config.json"))
        } else {
            // Fallback to current dir if no appdata available
            Some(PathBuf::from("mic_config.json"))
        }
    }

    /// Clamp and sanitize config values to valid ranges.
    fn validate(&mut self) {
        self.afk.timeout = self
            .afk
            .timeout
            .clamp(constants::MIN_AFK_TIMEOUT_S, constants::MAX_AFK_TIMEOUT_S);
        self.persistent_overlay.scale = self
            .persistent_overlay
            .scale
            .clamp(10, constants::MAX_OVERLAY_SCALE);
        if self.osd.duration == 0 {
            self.osd.duration = constants::DEFAULT_OSD_DURATION_MS;
        }
        if self.osd.size == 0 {
            self.osd.size = constants::DEFAULT_OSD_SIZE;
        }
        if self.audio_mode != "beep" && self.audio_mode != "custom" {
            self.audio_mode = "beep".to_string();
        }
        if self.hotkey_mode != "toggle" && self.hotkey_mode != "separate" {
            self.hotkey_mode = "toggle".to_string();
        }
    }

    pub fn load() -> Self {
        if let Some(path) = Self::get_config_path()
            && path.exists()
                && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(mut config) = serde_json::from_str::<Self>(&content) {
                        // Migrate legacy Python config formats
                        let mut needs_save = false;
                        if let Some(mode_val) = config.hotkey.remove("mode")
                            && let Some(mode_str) = mode_val.as_str() {
                                config.hotkey_mode = mode_str.to_string();
                                needs_save = true;
                            }

                        config.validate();

                        if needs_save {
                            let _ = config.save();
                        }

                        return config;
                    }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path =
            Self::get_config_path().ok_or_else(|| "Could not determine config path".to_string())?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Config serialization failed: {}", e))?;
        fs::write(&path, json).map_err(|e| {
            let msg = format!("Failed to write config to {}: {}", path.display(), e);
            tracing::error!("{}", msg);
            msg
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.hotkey_mode, "toggle");
        assert!(cfg.beep_enabled);
    }

    #[test]
    fn test_config_validation() {
        let mut cfg = AppConfig::default();
        cfg.osd.duration = 0;
        cfg.validate();
        assert_eq!(cfg.osd.duration, 1500);
    }
}
