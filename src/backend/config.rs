use crate::constants;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn default_theme() -> String {
    "Auto".to_string()
}

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
            enabled: true,
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
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for OsdConfig {
    fn default() -> Self {
        OsdConfig {
            enabled: true,
            duration: 1500,
            position: "Bottom".to_string(),
            size: 150,
            opacity: 80,
            theme: "Auto".to_string(),
        }
    }
}

/// Per-monitor overlay and OSD configurations keyed by sanitized monitor label.
/// The special key `"primary"` is used for the primary monitor as a default/fallback.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub device_id: Option<String>,
    pub sync_ids: Vec<String>,
    pub beep_enabled: bool,
    pub audio_mode: String, // "beep" or "custom"

    #[serde(rename = "beep_config")]
    pub beep_mode_configs: HashMap<String, BeepConfig>,
    #[serde(rename = "sound_config")]
    pub sound_mode_configs: HashMap<String, SoundConfig>,

    pub hotkey: HashMap<String, serde_json::Value>,
    pub hotkey_mode: String, // "toggle" or "separate"

    pub afk: AfkConfig,

    /// Keyed by sanitized monitor label (e.g., "primary", "__DISPLAY1").
    pub persistent_overlay: HashMap<String, OverlayConfig>,
    /// Keyed by sanitized monitor label.
    pub osd: HashMap<String, OsdConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut beep_mode_configs = HashMap::new();
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

        let mut sound_mode_configs = HashMap::new();
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

        let mut hotkey = HashMap::new();
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

        let mut persistent_overlay = HashMap::new();
        persistent_overlay.insert("primary".to_string(), OverlayConfig::default());

        let mut osd = HashMap::new();
        osd.insert("primary".to_string(), OsdConfig::default());

        Self {
            device_id: None,
            sync_ids: vec![],
            beep_enabled: true,
            audio_mode: "beep".to_string(),
            beep_mode_configs,
            sound_mode_configs,
            hotkey,
            hotkey_mode: "toggle".to_string(),
            afk: AfkConfig::default(),
            persistent_overlay,
            osd,
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
            Some(PathBuf::from("mic_config.json"))
        }
    }

    /// Clamp and sanitize config values to valid ranges.
    fn validate(&mut self) {
        self.afk.timeout = self
            .afk
            .timeout
            .clamp(constants::MIN_AFK_TIMEOUT_S, constants::MAX_AFK_TIMEOUT_S);

        for overlay_cfg in self.persistent_overlay.values_mut() {
            overlay_cfg.scale = overlay_cfg.scale.clamp(10, constants::MAX_OVERLAY_SCALE);
        }

        for osd_cfg in self.osd.values_mut() {
            if osd_cfg.duration == 0 {
                osd_cfg.duration = constants::DEFAULT_OSD_DURATION_MS;
            }
            if osd_cfg.size == 0 {
                osd_cfg.size = constants::DEFAULT_OSD_SIZE;
            }
        }

        if self.audio_mode != "beep" && self.audio_mode != "custom" {
            self.audio_mode = "beep".to_string();
        }
        if self.hotkey_mode != "toggle" && self.hotkey_mode != "separate" {
            self.hotkey_mode = "toggle".to_string();
        }
    }

    pub fn load() -> Self {
        let path = match Self::get_config_path() {
            Some(p) if p.exists() => p,
            _ => return Self::default(),
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, path = %path.display(), "Failed to read config file, using defaults");
                return Self::default();
            }
        };

        // Parse as raw JSON first so we can migrate old flat-format fields.
        let mut raw: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, path = %path.display(), "Failed to parse config JSON, using defaults");
                return Self::default();
            }
        };

        let mut needs_save = false;

        // Migrate old-format persistent_overlay (flat OverlayConfig → HashMap<String, OverlayConfig>)
        if raw.get("persistent_overlay").and_then(|v| v.get("enabled")).is_some() {
            let old = raw["persistent_overlay"].clone();
            raw["persistent_overlay"] = serde_json::json!({ "primary": old });
            needs_save = true;
            tracing::info!("Config migration: converting flat persistent_overlay to HashMap");
        }

        // Migrate old-format osd (flat OsdConfig → HashMap<String, OsdConfig>)
        if raw.get("osd").and_then(|v| v.get("enabled")).is_some() {
            let old = raw["osd"].clone();
            raw["osd"] = serde_json::json!({ "primary": old });
            needs_save = true;
            tracing::info!("Config migration: converting flat osd to HashMap");
        }

        let mut config: Self = match serde_json::from_value(raw) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Config deserialization failed after migration");
                return Self::default();
            }
        };

        // Migrate legacy Python config: hotkey.mode → hotkey_mode
        if let Some(mode_val) = config.hotkey.remove("mode")
            && let Some(mode_str) = mode_val.as_str() {
                tracing::info!(mode = mode_str, "Config migration: hotkey.mode → hotkey_mode");
                config.hotkey_mode = mode_str.to_string();
                needs_save = true;
            }

        // Ensure at least a "primary" entry exists for both maps
        if config.persistent_overlay.is_empty() {
            config.persistent_overlay.insert("primary".to_string(), OverlayConfig::default());
            needs_save = true;
        }
        if config.osd.is_empty() {
            config.osd.insert("primary".to_string(), OsdConfig::default());
            needs_save = true;
        }

        config.validate();

        if needs_save {
            match config.save() {
                Ok(()) => tracing::info!("Config migration saved successfully"),
                Err(e) => tracing::error!(error = %e, "Config migration save FAILED"),
            }
        }

        config
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
        assert!(cfg.persistent_overlay.contains_key("primary"));
        assert!(cfg.osd.contains_key("primary"));
    }

    #[test]
    fn test_config_validation() {
        let mut cfg = AppConfig::default();
        cfg.osd.get_mut("primary").unwrap().duration = 0;
        cfg.validate();
        assert_eq!(cfg.osd["primary"].duration, 1500);
    }

    #[test]
    fn test_migration_from_flat_overlay() {
        // Simulate old-format JSON
        let old_json = r#"{
            "persistent_overlay": { "enabled": true, "scale": 64, "x": 200, "y": 300,
                "show_vu": false, "opacity": 80, "position_mode": "Custom",
                "locked": false, "sensitivity": 5, "scale": 64, "theme": "Auto" },
            "osd": { "enabled": false, "duration": 1500, "position": "Bottom", "size": 150, "opacity": 80 }
        }"#;

        let mut raw: serde_json::Value = serde_json::from_str(old_json).unwrap();

        // Apply migration logic
        if raw.get("persistent_overlay").and_then(|v| v.get("enabled")).is_some() {
            let old = raw["persistent_overlay"].clone();
            raw["persistent_overlay"] = serde_json::json!({ "primary": old });
        }
        if raw.get("osd").and_then(|v| v.get("enabled")).is_some() {
            let old = raw["osd"].clone();
            raw["osd"] = serde_json::json!({ "primary": old });
        }

        let cfg: AppConfig = serde_json::from_value(raw).unwrap();
        assert!(cfg.persistent_overlay.contains_key("primary"));
        assert!(cfg.osd.contains_key("primary"));
        assert!(cfg.persistent_overlay["primary"].enabled);
        assert_eq!(cfg.persistent_overlay["primary"].x, 200);
    }
}
