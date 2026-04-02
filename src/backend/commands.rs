//! Tauri command handlers for the MicMuteRs application.
//!
//! This module contains all the command functions exposed to the frontend
//! via Tauri's IPC system. These commands handle audio control, configuration
//! management, and system integration.

use serde::Serialize;
use std::sync::Arc;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::{AppState, AudioMsg, config, startup};

/// Response structure for application state (mute + peak level).
#[derive(Serialize)]
pub struct AppStateDto {
    pub is_muted: bool,
    pub peak_level: f32,
}

/// Response structure for audio device list.
#[derive(Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
}

/// Get current mute state and VU peak level.
#[tauri::command]
pub async fn get_state(state: State<'_, Arc<AppState>>) -> Result<AppStateDto, String> {
    let is_muted = *state.is_muted.lock();
    let peak = state.peak_level.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10000.0;
    Ok(AppStateDto {
        is_muted,
        peak_level: peak,
    })
}

/// Toggle mic mute, return new state.
#[tauri::command]
pub async fn toggle_mute(
    _app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AppStateDto, String> {
    let cfg = state.config.lock().clone();
    let _ = state.audio_tx.try_send(AudioMsg::ToggleMute(cfg));

    // Return the current state (it will be updated asynchronously)
    let is_muted = *state.is_muted.lock();
    let peak = state.peak_level.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10000.0;
    Ok(AppStateDto {
        is_muted,
        peak_level: peak,
    })
}

/// Explicitly set mute state.
#[tauri::command]
pub async fn set_mute(
    _app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    muted: bool,
) -> Result<AppStateDto, String> {
    let cfg = state.config.lock().clone();
    let _ = state.audio_tx.try_send(AudioMsg::SetMute(muted, cfg));

    let peak = state.peak_level.load(std::sync::atomic::Ordering::Relaxed) as f32 / 10000.0;
    Ok(AppStateDto {
        is_muted: muted,
        peak_level: peak,
    })
}

/// Get full config.
#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<config::AppConfig, String> {
    Ok(state.config.lock().clone())
}

/// Save updated config, re-apply hotkeys, and sync overlay/OSD windows.
#[tauri::command]
pub async fn update_config(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    payload: String,
) -> Result<(), String> {
    let mut new_config: config::AppConfig = match serde_json::from_str(&payload) {
        Ok(cfg) => {
            tracing::debug!("Config deserialization successful");
            cfg
        }
        Err(e) => {
            tracing::error!(error = %e, "Config deserialization failed");
            return Err(format!("Config deserialization failed: {}", e));
        }
    };

    // Preserve per-monitor overlay positions
    {
        let mut cfg = state.config.lock();
        for (key, existing) in cfg.persistent_overlay.iter() {
            if let Some(new_overlay) = new_config.persistent_overlay.get_mut(key) {
                new_overlay.x = existing.x;
                new_overlay.y = existing.y;
            }
        }
        *cfg = new_config.clone();
    }
    if let Err(e) = new_config.save() {
        tracing::error!("{}", e);
    }

    // Update hotkeys — register ALL configured VKs regardless of mode.
    // The hook must always consume the key (prevent pass-through to Windows).
    // The hotkey loop in lib.rs routes based on mode, so registering extra
    // VKs is safe — they just won't trigger any action in the wrong mode.
    let get_vk = |val: &serde_json::Value| -> u32 {
        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    };
    let mut vks: Vec<u32> = Vec::new();
    for key in &["toggle", "mute", "unmute"] {
        if let Some(h) = new_config.hotkey.get(*key) {
            let v = get_vk(h);
            if v != 0 && !vks.contains(&v) {
                vks.push(v);
            }
        }
    }
    {
        let hotkeys = state.hotkeys.lock();
        hotkeys.set_hotkeys(vks);
    }

    // Sync windows — called directly from the async command thread.
    // WebviewWindowBuilder::build() internally dispatches to the event loop,
    // which works from a background thread but deadlocks inside run_on_main_thread.
    let monitors = crate::get_monitor_info(&app);

    // Rebuild tray menu checkmarks
    if let Some(tray) = app.tray_by_id("main") {
        let devices = state.available_devices.lock().clone();
        if let Ok(menu) = crate::build_tray_menu(&app, &new_config, &devices) {
            let _ = tray.set_menu(Some(menu));
        }
    }

    crate::sync_overlay_windows(&app, &new_config, &monitors);
    crate::sync_osd_windows(&app, &new_config, &monitors);

    // Emit config update to all frontend windows
    use tauri::Emitter;
    let _ = app.emit(
        "config-update",
        serde_json::json!({
            "config": new_config
        }),
    );

    Ok(())
}

/// Enumerate audio capture devices.
#[tauri::command]
pub async fn get_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    let _ = state.audio_tx.try_send(AudioMsg::RefreshDevices);
    // Give it a tiny moment to refresh
    std::thread::sleep(std::time::Duration::from_millis(50));
    let devs = state.available_devices.lock().clone();
    Ok(devs
        .into_iter()
        .map(|(id, name)| DeviceDto { id, name })
        .collect())
}

/// Get the current cached device list.
#[tauri::command]
pub async fn get_cached_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    let devs = state.available_devices.lock().clone();
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
    let _ = state.audio_tx.try_send(AudioMsg::SetDevice(device_id.clone()));
    let mut cfg = state.config.lock();
    cfg.device_id = device_id;
    if let Err(e) = cfg.save() {
        tracing::error!("{}", e);
    }
    Ok(())
}

/// Start listening for a single keypress to record as a hotkey.
#[tauri::command]
pub async fn start_recording_hotkey(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let hotkeys = state.hotkeys.lock();
    hotkeys.start_recording();
    Ok(())
}

/// Stop listening for hotkey recording.
#[tauri::command]
pub async fn stop_recording_hotkey(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let hotkeys = state.hotkeys.lock();
    hotkeys.stop_recording();
    Ok(())
}

/// Check if a key has been recorded. Returns the VK code or 0.
#[tauri::command]
pub async fn get_recorded_hotkey(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let hotkeys = state.hotkeys.lock();
    Ok(hotkeys.try_recv_record().unwrap_or(0))
}

/// Enable or disable "run on startup" via Windows Task Scheduler.
/// Returns the actual scheduler state after the operation so the UI can
/// confirm whether it succeeded, and rebuilds the tray menu to keep it in sync.
#[tauri::command]
pub async fn set_run_on_startup_cmd(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    enable: bool,
) -> Result<bool, String> {
    startup::set_run_on_startup(enable);
    let actual = startup::get_run_on_startup();
    // Rebuild tray menu so the checkmark reflects the new state
    let cfg = state.config.lock().clone();
    crate::sync_tray_and_emit(&app, &state, &cfg);
    Ok(actual)
}

/// Get current "run on startup" status.
#[tauri::command]
pub async fn get_run_on_startup_cmd() -> Result<bool, String> {
    Ok(startup::get_run_on_startup())
}

/// Preview a sound based on current UI state.
#[tauri::command]
pub async fn preview_audio_feedback(
    state: State<'_, Arc<AppState>>,
    mode: String,
    key: String,
    payload: String,
) -> Result<(), String> {
    const MAX_CONFIG_SIZE: usize = 64 * 1024;
    if payload.len() > MAX_CONFIG_SIZE {
        return Err("Payload too large".into());
    }
    let temp_config: config::AppConfig =
        serde_json::from_str(&payload).map_err(|e| e.to_string())?;

    let _ = state.audio_tx.try_send(AudioMsg::PlayPreview(mode, key, temp_config));
    Ok(())
}

/// Open a URL in the default browser. Only http/https URLs are allowed.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only http/https URLs are allowed".to_string());
    }
    open::that(&url).map_err(|e| e.to_string())
}

/// Triggers a file picker via Tauri's dialog plugin.
#[tauri::command]
pub async fn pick_audio_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file_path = app.dialog()
        .file()
        .add_filter("Audio Files", &["wav", "mp3", "ogg"])
        .blocking_pick_file();
    
    Ok(file_path.map(|p| p.to_string()))
}

/// Check if the background behind a specific overlay window is light or dark.
/// `window_label` identifies which overlay window to sample (e.g., "overlay", "overlay-2").
#[tauri::command]
pub async fn get_overlay_background_is_light(
    app: tauri::AppHandle,
    window_label: Option<String>,
) -> Result<bool, String> {
    use windows::Win32::Foundation::HWND;

    let label = window_label.unwrap_or_else(|| "overlay".to_string());

    if let Some(overlay_win) = app.get_webview_window(&label) {
        let tauri_hwnd = overlay_win.hwnd().map_err(|e: tauri::Error| e.to_string())?;
        let hwnd = HWND(tauri_hwnd.0);
        Ok(crate::theme::is_background_light(hwnd))
    } else {
        Ok(crate::theme::is_system_light_theme())
    }
}

/// Return the overlay always-on-top re-assertion interval in milliseconds.
#[tauri::command]
pub fn get_overlay_topmost_interval() -> u64 {
    crate::constants::OVERLAY_TOPMOST_INTERVAL_MS
}

/// Save the current overlay window position to config without a full config update.
/// Called when the user finishes dragging a specific overlay window.
#[tauri::command]
pub async fn save_overlay_position(
    state: State<'_, Arc<AppState>>,
    monitor_key: String,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let cfg_to_save = {
        let mut cfg = state.config.lock();
        if let Some(overlay_cfg) = cfg.persistent_overlay.get_mut(&monitor_key) {
            overlay_cfg.x = x;
            overlay_cfg.y = y;
        } else {
            tracing::warn!(monitor_key = %monitor_key, "save_overlay_position: unknown monitor key");
            return Ok(());
        }
        cfg.clone()
    };
    if let Err(e) = cfg_to_save.save() {
        tracing::error!("{}", e);
    }
    Ok(())
}

/// Return all connected monitors with their geometry and a sanitized label key.
/// The primary monitor is always returned with label_key "primary" to match
/// the special config key.
#[tauri::command]
pub async fn get_monitors(app: tauri::AppHandle) -> Result<Vec<crate::MonitorInfo>, String> {
    Ok(crate::get_monitor_info(&app))
}

/// Return the monitor config key assigned to a given window label.
/// Used by overlay.js to determine which per-monitor config entry applies.
#[tauri::command]
pub fn get_window_monitor_key(label: String) -> Option<String> {
    crate::window_monitor_key(&label)
}
