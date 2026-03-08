//! Tauri command handlers for the MicMuteRs application.
//!
//! This module contains all the command functions exposed to the frontend
//! via Tauri's IPC system. These commands handle audio control, configuration
//! management, and system integration.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::{AppState, audio, config, startup};

// ─────────────────────────────────────────
//  Response types
// ─────────────────────────────────────────
#[derive(Serialize, Clone)]
pub struct AppStateDto {
    pub is_muted: bool,
    pub peak_level: f32,
}

#[derive(Serialize, Clone)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
}

// ─────────────────────────────────────────
//  Commands
// ─────────────────────────────────────────

/// Get current mute state and VU peak level.
#[tauri::command]
pub async fn get_state(state: State<'_, Arc<AppState>>) -> Result<AppStateDto, String> {
    let is_muted = *state.is_muted.lock().unwrap();
    let peak = state.audio.lock().unwrap().get_peak_value().unwrap_or(0.0);
    if peak > 0.0001 {
        tracing::debug!(
            peak_level = peak,
            is_muted = is_muted,
            "get_state called with significant peak level"
        );
    }
    Ok(AppStateDto {
        is_muted,
        peak_level: peak,
    })
}

/// Toggle mic mute, return new state.
#[tauri::command]
pub async fn toggle_mute(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AppStateDto, String> {
    let cfg = state.config.lock().unwrap().clone();
    let (muted, peak, stream_handle) = {
        let audio = state.audio.lock().unwrap();
        let (m, _) = audio.toggle_mute(&cfg).map_err(|e| e.to_string())?;
        let p = audio.get_peak_value().unwrap_or(0.0);
        let sh = audio.stream_handle();
        (m, p, sh)
    };
    *state.is_muted.lock().unwrap() = muted;
    crate::update_tray_icon(&app, muted);
    crate::emit_state(&app, muted, peak);
    crate::trigger_osd(&app, muted);

    std::thread::spawn(move || {
        crate::audio::play_feedback(&stream_handle, muted, &cfg);
    });

    Ok(AppStateDto {
        is_muted: muted,
        peak_level: peak,
    })
}

/// Explicitly set mute state.
#[tauri::command]
pub async fn set_mute(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    muted: bool,
) -> Result<AppStateDto, String> {
    let cfg = state.config.lock().unwrap().clone();
    let (success, peak, stream_handle) = {
        let audio = state.audio.lock().unwrap();
        if audio.set_mute(muted, &cfg).is_ok() {
            let p = audio.get_peak_value().unwrap_or(0.0);
            (true, p, audio.stream_handle())
        } else {
            (false, 0.0, audio.stream_handle())
        }
    };

    if success {
        *state.is_muted.lock().unwrap() = muted;
        crate::update_tray_icon(&app, muted);
        crate::emit_state(&app, muted, peak);
        crate::trigger_osd(&app, muted);

        std::thread::spawn(move || {
            crate::audio::play_feedback(&stream_handle, muted, &cfg);
        });

        Ok(AppStateDto {
            is_muted: muted,
            peak_level: peak,
        })
    } else {
        Err("Failed to set mute".to_string())
    }
}

/// Get full config.
#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<config::AppConfig, String> {
    Ok(state.config.lock().unwrap().clone())
}

/// Save updated config, re-apply hotkeys.
#[tauri::command]
pub async fn update_config(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    payload: String,
) -> Result<(), String> {
    tracing::debug!(payload_len = payload.len(), "update_config called");
    let new_config: config::AppConfig = match serde_json::from_str(&payload) {
        Ok(cfg) => {
            tracing::debug!("Config deserialization successful");
            cfg
        }
        Err(e) => {
            tracing::error!(error = %e, "Config deserialization failed");
            return Err(format!("Config deserialization failed: {}", e));
        }
    };
    new_config.save();
    let get_vk = |val: &serde_json::Value| -> u32 {
        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    };
    let mut vks: Vec<u32> = Vec::new();
    let mode = new_config.hotkey_mode.as_str();
    if mode == "toggle" {
        if let Some(h) = new_config.hotkey.get("toggle") {
            let v = get_vk(h);
            if v != 0 {
                vks.push(v);
            }
        }
    } else {
        if let Some(h) = new_config.hotkey.get("mute") {
            let v = get_vk(h);
            if v != 0 {
                vks.push(v);
            }
        }
        if let Some(h) = new_config.hotkey.get("unmute") {
            let v = get_vk(h);
            if v != 0 {
                vks.push(v);
            }
        }
    }
    {
        let hotkeys = state.hotkeys.lock().unwrap();
        hotkeys.set_hotkeys(vks);
    }
    *state.config.lock().unwrap() = new_config.clone();

    // UPDATE TRAY MENU Checkmarks
    use tauri::Manager;
    if let Some(tray) = app.tray_by_id("main") {
        let devices = state.available_devices.lock().unwrap().clone();
        let menu = crate::build_tray_menu(&app, &new_config, &devices);
        let _ = tray.set_menu(Some(menu));
    }

    // UPDATE OVERLAY WINDOW position, scale, visibility
    if let Some(win) = app.get_webview_window("overlay") {
        if new_config.persistent_overlay.enabled {
            let scale = new_config.persistent_overlay.scale as f64;
            let w = if new_config.persistent_overlay.show_vu {
                scale + 30.0
            } else {
                scale
            };
            let _ = win.set_size(tauri::LogicalSize::new(w, scale));
            let _ = win.set_position(tauri::LogicalPosition::new(
                new_config.persistent_overlay.x as f64,
                new_config.persistent_overlay.y as f64,
            ));
            let _ = win.set_ignore_cursor_events(new_config.persistent_overlay.locked);
            let _ = win.show();
        } else {
            let _ = win.hide();
        }
    }

    // EMIT CONFIG UPDATE EVENT so all frontend windows sync up
    use tauri::Emitter;
    let _ = app.emit(
        "config-update",
        serde_json::json!({
            "config": new_config
        }),
    );

    Ok(())
}

/// Return cached audio devices from application state (no COM enumeration).
/// This is used for initial UI load to avoid COM threading issues.
#[tauri::command]
pub async fn get_cached_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    let devs = state.available_devices.lock().unwrap().clone();
    eprintln!(
        "[DEBUG] get_cached_devices: returning {} devices",
        devs.len()
    );
    for (id, name) in &devs {
        eprintln!("[DEBUG]   device: {} -> {}", name, id);
    }
    Ok(devs
        .into_iter()
        .map(|(id, name)| DeviceDto { id, name })
        .collect())
}

/// Enumerate audio capture devices (fresh COM enumeration).
/// Used by "Refresh" button. Falls back to cached devices if enumeration fails.
#[tauri::command]
pub async fn get_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    let devs = match audio::get_audio_devices() {
        Ok(d) if !d.is_empty() => {
            *state.available_devices.lock().unwrap() = d.clone();
            d
        }
        Ok(_) | Err(_) => state.available_devices.lock().unwrap().clone(),
    };
    Ok(devs
        .into_iter()
        .map(|(id, name)| DeviceDto { id, name })
        .collect())
}

/// Switch the active audio device.
#[tauri::command]
pub async fn set_device(
    state: State<'_, Arc<AppState>>,
    device_id: Option<String>,
) -> Result<(), String> {
    let new_audio = audio::AudioController::new(device_id.as_ref()).map_err(|e| e.to_string())?;
    *state.audio.lock().unwrap() = new_audio;
    let mut cfg = state.config.lock().unwrap();
    cfg.device_id = device_id;
    cfg.save();
    Ok(())
}

/// Begin hotkey recording mode.
#[tauri::command]
pub async fn start_recording_hotkey(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.hotkeys.lock().unwrap().start_recording();
    Ok(())
}

/// Poll for a recorded hotkey VK code (returns None if not yet recorded).
#[tauri::command]
pub async fn get_recorded_hotkey(state: State<'_, Arc<AppState>>) -> Result<Option<u32>, String> {
    Ok(state.hotkeys.lock().unwrap().try_recv_record())
}

/// Enable or disable run on startup.
#[tauri::command]
pub async fn set_run_on_startup_cmd(enable: bool) -> Result<(), String> {
    startup::set_run_on_startup(enable);
    Ok(())
}

/// Check whether run-on-startup is enabled.
#[tauri::command]
pub async fn get_run_on_startup_cmd() -> Result<bool, String> {
    Ok(startup::get_run_on_startup())
}

/// Open a file dialog to pick a WAV/MP3 file.
#[tauri::command]
pub async fn pick_audio_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();

    app.dialog()
        .file()
        .add_filter("Audio", &["wav", "mp3"])
        .pick_file(move |file_path| {
            let path = file_path.map(|p| p.to_string());
            let _ = tx.send(path);
        });

    rx.recv().map_err(|e| e.to_string())
}

/// Preview a sound based on current UI state (not yet saved to disk).
#[tauri::command]
pub async fn preview_audio_feedback(
    state: State<'_, Arc<AppState>>,
    mode: String,
    key: String,
    payload: String,
) -> Result<(), String> {
    let temp_config: config::AppConfig = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
    let stream_handle = state.audio.lock().unwrap().stream_handle();

    // Force mode for preview
    let mut preview_cfg = temp_config;
    preview_cfg.audio_mode = mode;

    std::thread::spawn(move || {
        crate::audio::play_feedback(&stream_handle, key == "mute", &preview_cfg);
    });

    Ok(())
}

/// Open a URL in the default browser.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}
