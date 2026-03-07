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
    let audio = state.audio.lock().unwrap();
    let (muted, _) = audio.toggle_mute(&cfg).map_err(|e| e.to_string())?;
    drop(audio);
    *state.is_muted.lock().unwrap() = muted;
    let peak = state.audio.lock().unwrap().get_peak_value().unwrap_or(0.0);
    crate::update_tray_icon(&app, muted);
    crate::emit_state(&app, muted, peak);
    crate::trigger_osd(&app, muted);
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
    let audio = state.audio.lock().unwrap();
    audio.set_mute(muted, &cfg).map_err(|e| e.to_string())?;
    audio.play_feedback(muted, &cfg);
    drop(audio);
    *state.is_muted.lock().unwrap() = muted;
    let peak = state.audio.lock().unwrap().get_peak_value().unwrap_or(0.0);
    crate::update_tray_icon(&app, muted);
    crate::emit_state(&app, muted, peak);
    crate::trigger_osd(&app, muted);
    Ok(AppStateDto {
        is_muted: muted,
        peak_level: peak,
    })
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
    new_config: config::AppConfig,
) -> Result<(), String> {
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
    state.hotkeys.lock().unwrap().set_hotkeys(vks);
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
                scale + 20.0
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

    Ok(())
}

/// Enumerate audio capture devices.
#[tauri::command]
pub async fn get_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    let devs = audio::get_audio_devices().map_err(|e| e.to_string())?;
    *state.available_devices.lock().unwrap() = devs.clone();
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

/// Open a URL in the default browser.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}
