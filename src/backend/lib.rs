pub mod audio;
pub mod com_interfaces;
pub mod commands;
pub mod config;
pub mod constants;
pub mod hotkey;
pub mod startup;
pub mod theme;
pub mod utils;

use crate::constants::DEFAULT_HOTKEY_VK;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use tauri::{
    AppHandle, Emitter, Manager,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Message sent to the audio worker thread.
pub enum AudioMsg {
    /// Toggle mute state. Uses the provided config for sync logic.
    ToggleMute(config::AppConfig),
    /// Set explicit mute state.
    SetMute(bool, config::AppConfig),
    /// Change the active audio device.
    SetDevice(Option<String>),
    /// Play a sound preview. (mode, key, config)
    PlayPreview(String, String, config::AppConfig),
    /// Refresh the list of available audio devices.
    RefreshDevices,
}

// ─────────────────────────────────────────
//  Shared application state
// ─────────────────────────────────────────
pub struct AppState {
    pub config: Mutex<config::AppConfig>,
    pub hotkeys: Mutex<hotkey::HotkeyManager>,
    pub is_muted: Mutex<bool>,
    /// Latest peak level (0.0 to 1.0) stored as fixed-point (0 to 10000).
    pub peak_level: std::sync::atomic::AtomicU32,
    pub available_devices: Mutex<Vec<(String, String)>>,
    pub audio_tx: std_mpsc::SyncSender<AudioMsg>,
    pub tray: Mutex<Option<tauri::tray::TrayIcon>>,
}

/// # Safety Invariants
///
/// 1. All COM interfaces are managed by the dedicated audio worker thread.
/// 2. `OutputStream` is kept on the worker thread and never moved.
/// 3. Communication with the audio worker is via a thread-safe SyncSender.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

// ─────────────────────────────────────────
//  Monitor helpers
// ─────────────────────────────────────────

/// Sanitize a monitor name to be a valid Tauri window label component.
/// Replaces non-alphanumeric characters (except hyphens) with underscores.
pub fn sanitize_label(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}

/// Snapshot of monitor properties to avoid blocking calls on the main thread.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub label_key: String,
    pub is_primary: bool,
    pub position: tauri::PhysicalPosition<i32>,
    pub size: tauri::PhysicalSize<u32>,
    pub scale_factor: f64,
}

/// Gather information about all currently connected monitors.
pub fn get_monitor_info(app: &AppHandle) -> Vec<MonitorInfo> {
    let monitors = app.available_monitors().unwrap_or_default();
    let primary = app.primary_monitor().ok().flatten();
    let primary_name = primary.as_ref().and_then(|m| m.name()).map(|n| n.to_string());

    let mut infos: Vec<MonitorInfo> = monitors
        .into_iter()
        .map(|m| {
            let name = m.name().map(|n| n.as_str()).unwrap_or("Unknown").to_string();
            let is_primary = primary_name.as_deref() == Some(&name);
            let label_key = if is_primary {
                "primary".to_string()
            } else {
                sanitize_label(&name)
            };
            MonitorInfo {
                name,
                label_key,
                is_primary,
                position: *m.position(),
                size: *m.size(),
                scale_factor: m.scale_factor(),
            }
        })
        .collect();

    // If no monitor was detected as primary (name mismatch between
    // primary_monitor() and available_monitors()), treat the first monitor
    // as primary so that the "primary" config key gets used.
    if !infos.is_empty() && !infos.iter().any(|m| m.is_primary) {
        tracing::debug!(
            primary_name = ?primary_name,
            first_monitor = %infos[0].name,
            "Primary monitor name mismatch — treating first monitor as primary"
        );
        infos[0].is_primary = true;
        infos[0].label_key = "primary".to_string();
    }

    infos
}

// ─────────────────────────────────────────
//  Static window label pools
// ─────────────────────────────────────────

/// Fixed overlay window labels defined in tauri.conf.json.
/// Index 0 = primary monitor, index 1 = secondary monitor.
const OVERLAY_LABELS: &[&str] = &["overlay", "overlay-2"];
/// Fixed OSD window labels defined in tauri.conf.json.
const OSD_LABELS: &[&str] = &["osd", "osd-2"];

/// Global mapping: window label → monitor config key.
/// Populated by `sync_overlay_windows` / `sync_osd_windows` and queried by
/// the `get_window_monitor_key` command so that overlay.js/osd.js know which
/// per-monitor config entry applies to their window.
static WINDOW_MONITOR_MAP: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

fn get_window_monitor_map(
) -> &'static parking_lot::Mutex<std::collections::HashMap<String, String>> {
    WINDOW_MONITOR_MAP.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()))
}

/// Return the monitor config key assigned to a given window label.
pub fn window_monitor_key(label: &str) -> Option<String> {
    get_window_monitor_map().lock().get(label).cloned()
}

// ─────────────────────────────────────────
//  Overlay window management
// ─────────────────────────────────────────

/// Apply sizing and click-through settings to an existing overlay window.
fn apply_overlay_config(win: &tauri::WebviewWindow, cfg: &config::OverlayConfig) {
    let scale = cfg.scale as f64;
    let w = if cfg.show_vu { scale + 30.0 } else { scale };
    let _ = win.set_size(tauri::LogicalSize::new(w, scale));
    let _ = win.set_ignore_cursor_events(true);
    if !cfg.locked
        && let Ok(tauri_hwnd) = win.hwnd() {
            use windows::Win32::Foundation::HWND;
            let hwnd = HWND(tauri_hwnd.0);
            crate::utils::set_click_through(hwnd, false);
        }
    let _ = win.set_always_on_top(true);
}

/// Show/hide/configure overlay windows for every connected monitor.
/// Maps monitors to static window labels by index (primary first).
/// Updates the global window→monitor mapping so overlay.js can query its config key.
pub fn sync_overlay_windows(app: &AppHandle, config: &config::AppConfig, monitors: &[MonitorInfo]) {
    // Sort monitors: primary first, then others.
    let mut sorted: Vec<&MonitorInfo> = monitors.iter().collect();
    sorted.sort_by_key(|m| !m.is_primary);

    let mut map = get_window_monitor_map().lock();

    for (idx, mon) in sorted.iter().enumerate() {
        if idx >= OVERLAY_LABELS.len() {
            break;
        }
        let label = OVERLAY_LABELS[idx];
        let key = &mon.label_key;
        map.insert(label.to_string(), key.clone());

        let overlay_cfg = config.persistent_overlay.get(key)
            .or_else(|| config.persistent_overlay.get("primary"))
            .cloned()
            .unwrap_or_default();

        if let Some(win) = app.get_webview_window(label) {
            if overlay_cfg.enabled {
                apply_overlay_config(&win, &overlay_cfg);
                if overlay_cfg.x != 0 || overlay_cfg.y != 0 {
                    let _ = win.set_position(tauri::PhysicalPosition::new(
                        overlay_cfg.x,
                        overlay_cfg.y,
                    ));
                } else {
                    let _ = win.set_position(tauri::PhysicalPosition::new(
                        mon.position.x + 100,
                        mon.position.y + 100,
                    ));
                }
                let _ = win.show();
            } else {
                let _ = win.hide();
            }
        }
    }

    // Hide unused static overlay windows (more labels than monitors).
    for label in OVERLAY_LABELS.iter().skip(sorted.len()) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.hide();
        }
    }
}

// ─────────────────────────────────────────
//  OSD window management
// ─────────────────────────────────────────

/// Show/hide OSD windows for enabled monitors.
/// Maps monitors to static OSD window labels by index (primary first).
/// Updates the global window→monitor mapping.
pub fn sync_osd_windows(app: &AppHandle, config: &config::AppConfig, monitors: &[MonitorInfo]) {
    let mut sorted: Vec<&MonitorInfo> = monitors.iter().collect();
    sorted.sort_by_key(|m| !m.is_primary);

    let mut map = get_window_monitor_map().lock();

    for (idx, mon) in sorted.iter().enumerate() {
        if idx >= OSD_LABELS.len() {
            break;
        }
        let label = OSD_LABELS[idx];
        let key = &mon.label_key;
        map.insert(label.to_string(), key.clone());

        let osd_cfg = config.osd.get(key)
            .or_else(|| config.osd.get("primary"))
            .cloned()
            .unwrap_or_default();
        if !osd_cfg.enabled
            && let Some(win) = app.get_webview_window(label) {
                let _ = win.hide();
            }
    }

    // Hide unused static OSD windows.
    for label in OSD_LABELS.iter().skip(sorted.len()) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.hide();
        }
    }
}

// ─────────────────────────────────────────
//  Per-monitor OSD hide timer
// ─────────────────────────────────────────

struct OsdTimer {
    tx: std_mpsc::Sender<OsdHideMsg>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

struct OsdHideMsg {
    win: tauri::WebviewWindow,
    delay: std::time::Duration,
    generation: u64,
}

impl OsdTimer {
    fn new() -> Self {
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let gen_clone = Arc::clone(&generation);
        let (tx, rx) = std_mpsc::channel::<OsdHideMsg>();
        std::thread::Builder::new()
            .name("osd-timer".into())
            .spawn(move || {
                while let Ok(mut msg) = rx.recv() {
                    // Drain any queued messages, keeping only the latest one.
                    // Without this, a burst of schedule_hide calls would cause
                    // the worker to sleep N×delay instead of just delay.
                    while let Ok(newer) = rx.try_recv() {
                        msg = newer;
                    }
                    std::thread::sleep(msg.delay);
                    if gen_clone.load(std::sync::atomic::Ordering::SeqCst) == msg.generation {
                        let _ = msg.win.hide();
                    }
                }
            })
            .expect("failed to spawn OSD timer thread");
        Self { tx, generation }
    }

    fn schedule_hide(&self, win: tauri::WebviewWindow, delay: std::time::Duration) {
        let new_gen = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let _ = self.tx.send(OsdHideMsg { win, delay, generation: new_gen });
    }
}

/// Per-monitor OSD timer registry. Keyed by OSD window label (e.g., "osd-primary").
static OSD_TIMERS: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<String, OsdTimer>>,
> = std::sync::OnceLock::new();

fn get_osd_timers() -> &'static parking_lot::Mutex<std::collections::HashMap<String, OsdTimer>> {
    OSD_TIMERS.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()))
}

// ─────────────────────────────────────────
//  Tray helpers
// ─────────────────────────────────────────
pub(crate) fn build_tray_menu<M: Manager<tauri::Wry>>(
    app: &M,
    cfg: &config::AppConfig,
    devices: &[(String, String)],
) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let menu = Menu::new(app)?;

    let toggle_item =
        MenuItem::with_id(app, "toggle_mute", "Toggle Mute", true, None::<&str>)?;
    let _ = menu.append(&toggle_item);
    let _ = menu.append(&PredefinedMenuItem::separator(app)?);

    // Microphone submenu
    let mic_menu = Submenu::new(app, "Select Microphone", true)?;
    let default_item = CheckMenuItem::with_id(
        app,
        "mic_default",
        "Default Windows Device",
        true,
        cfg.device_id.is_none(),
        None::<&str>,
    )?;
    let _ = mic_menu.append(&default_item);
    for (id, name) in devices {
        let is_sel = cfg.device_id.as_ref() == Some(id);
        let key = format!("mic_{}", id);
        let item = CheckMenuItem::with_id(app, key, name, true, is_sel, None::<&str>)?;
        let _ = mic_menu.append(&item);
    }
    let _ = menu.append(&mic_menu);

    let _ = menu.append(&PredefinedMenuItem::separator(app)?);

    // Tray checkmarks reflect whether ANY monitor has the feature enabled
    let osd_any_enabled = cfg.osd.values().any(|o| o.enabled);
    let overlay_any_enabled = cfg.persistent_overlay.values().any(|o| o.enabled);

    let sound_item = CheckMenuItem::with_id(
        app,
        "toggle_sound",
        "Play Sound on Toggle",
        true,
        cfg.beep_enabled,
        None::<&str>,
    )?;
    let osd_item = CheckMenuItem::with_id(
        app,
        "toggle_osd",
        "Enable OSD Notification",
        true,
        osd_any_enabled,
        None::<&str>,
    )?;
    let overlay_item = CheckMenuItem::with_id(
        app,
        "toggle_overlay",
        "Show Persistent Overlay",
        true,
        overlay_any_enabled,
        None::<&str>,
    )?;
    let boot_item = CheckMenuItem::with_id(
        app,
        "toggle_boot",
        "Start on Boot",
        true,
        startup::get_run_on_startup(),
        None::<&str>,
    )?;

    let _ = menu.append_items(&[
        &sound_item,
        &osd_item,
        &overlay_item,
        &boot_item,
        &PredefinedMenuItem::separator(app)?,
        &MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?,
        &MenuItem::with_id(app, "help", "Help", true, None::<&str>)?,
        &MenuItem::with_id(app, "about", "About", true, None::<&str>)?,
        &PredefinedMenuItem::separator(app)?,
        &MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?,
    ]);

    Ok(menu)
}

pub fn load_tray_icon(is_muted: bool, is_light: bool) -> Result<Image<'static>, tauri::Error> {
    let bytes: &[u8] = match (is_muted, is_light) {
        (true, true) => include_bytes!("../frontend/assets/mic_muted_black.ico"),
        (false, true) => include_bytes!("../frontend/assets/mic_black.ico"),
        (true, false) => include_bytes!("../frontend/assets/mic_muted_white.ico"),
        (false, false) => include_bytes!("../frontend/assets/mic_white.ico"),
    };
    Image::from_bytes(bytes)
}

// ─────────────────────────────────────────
//  Emit helper
// ─────────────────────────────────────────
pub fn emit_state(app: &AppHandle, is_muted: bool, peak: f32) {
    let _ = app.emit(
        "state-update",
        serde_json::json!({
            "is_muted": is_muted,
            "peak_level": peak,
        }),
    );
}

fn spawn_audio_worker(
    app: AppHandle,
    state: Arc<AppState>,
    rx: std_mpsc::Receiver<AudioMsg>,
    initial_device_id: Option<String>,
) {
    std::thread::Builder::new()
        .name("audio-worker".into())
        .spawn(move || {
            // Initialize COM for this thread (Multithreaded Apartment)
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                );
            }

            let mut controller = match audio::AudioController::new(initial_device_id.as_ref()) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        "Failed to initialize audio controller, falling back to default"
                    );
                    audio::AudioController::new(None).ok()
                }
            };

            if controller.is_none() {
                tracing::error!("Failed to initialize audio controller and fallback failed. Audio features will be disabled.");
            } else {
                tracing::info!("Audio worker: controller initialized OK");
            }

            let mut _active_sink: Option<rodio::Sink> = None;
            let mut last_peak_poll = std::time::Instant::now();
            let mut current_muted = controller.as_ref().map(|c| c.is_muted().unwrap_or(false)).unwrap_or(false);
            {
                *state.is_muted.lock() = current_muted;
            }

            loop {
                // Check for messages with a short timeout to allow for periodic peak polling
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(msg) => match msg {
                        AudioMsg::ToggleMute(cfg) => {
                            tracing::info!("Audio worker: received ToggleMute");
                            if let Some(ref mut c) = controller {
                                match c.toggle_mute(&cfg) {
                                    Ok(new_muted) => {
                                        tracing::info!(muted = new_muted, "Audio worker: mute toggled");
                                        current_muted = new_muted;
                                        let peak = c.get_peak_value().unwrap_or(0.0);
                                        finalize_mute_change(&app, &state, new_muted, peak, &cfg);
                                        _active_sink = audio::play_feedback(
                                            c.stream_handle().as_ref(),
                                            new_muted,
                                            &cfg,
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!(error = ?e, "Audio worker: toggle_mute COM call failed");
                                    }
                                }
                            } else {
                                tracing::error!("Audio worker: ToggleMute received but controller is None");
                            }
                        }
                        AudioMsg::SetMute(mute, cfg) => {
                            if let Some(ref mut c) = controller
                                && c.set_mute(mute, &cfg).is_ok() {
                                    current_muted = mute;
                                    let peak = c.get_peak_value().unwrap_or(0.0);
                                    finalize_mute_change(&app, &state, mute, peak, &cfg);
                                    _active_sink =
                                        audio::play_feedback(c.stream_handle().as_ref(), mute, &cfg);
                                }
                        }
                        AudioMsg::SetDevice(id) => {
                            if let Ok(new_ctrl) = audio::AudioController::new(id.as_ref()) {
                                controller = Some(new_ctrl);
                                current_muted = controller.as_ref().map(|c| c.is_muted().unwrap_or(false)).unwrap_or(false);
                                {
                                    *state.is_muted.lock() = current_muted;
                                }
                                // Refresh devices list after switching
                                if let Ok(devices) = audio::get_audio_devices() {
                                    {
                                        *state.available_devices.lock() = devices;
                                    }
                                }
                            }
                        }
                        AudioMsg::PlayPreview(mode, key, mut cfg) => {
                            if let Some(ref c) = controller {
                                cfg.audio_mode = mode;
                                _active_sink = audio::play_feedback(
                                    c.stream_handle().as_ref(),
                                    key == "mute",
                                    &cfg,
                                );
                            }
                        }
                        AudioMsg::RefreshDevices => {
                            if let Ok(devices) = audio::get_audio_devices() {
                                *state.available_devices.lock() = devices;
                            }
                        }
                    },
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {
                        // Periodic peak level polling (~10Hz)
                        if last_peak_poll.elapsed() >= std::time::Duration::from_millis(100) {
                            if let Some(ref c) = controller
                                && let Ok(peak) = c.get_peak_value() {
                                    state.peak_level.store(
                                        (peak * 10000.0) as u32,
                                        std::sync::atomic::Ordering::Relaxed,
                                    );
                                    // Emit state update periodically for VU meters
                                    emit_state(&app, current_muted, peak);
                                }
                            last_peak_poll = std::time::Instant::now();
                        }
                    }
                    Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("failed to spawn audio worker thread");
}

fn finalize_mute_change(
    app: &AppHandle,
    state: &Arc<AppState>,
    muted: bool,
    peak: f32,
    cfg: &config::AppConfig,
) {
    tracing::info!(muted = muted, "finalize_mute_change: start");
    *state.is_muted.lock() = muted;
    state
        .peak_level
        .store((peak * 10000.0) as u32, std::sync::atomic::Ordering::Relaxed);

    // Emit state event — thread-safe, updates overlay VU/icon via JS listener
    emit_state(app, muted, peak);

    // Dispatch tray + OSD updates to the main thread.
    // Keep the callback lightweight — no window creation, only show/hide/emit.
    let app_clone = app.clone();
    let cfg_clone = cfg.clone();
    let _ = app.run_on_main_thread(move || {
        tracing::info!(muted = muted, "finalize_mute_change: on main thread");
        update_tray_icon(&app_clone, muted);

        let monitors = get_monitor_info(&app_clone);
        trigger_osd(&app_clone, muted, &cfg_clone, &monitors);
        tracing::info!("finalize_mute_change: done");
    });
}

fn init_hotkeys(cfg: &config::AppConfig) -> hotkey::HotkeyManager {
    let mut initial_vks: Vec<u32> = Vec::new();
    let get_vk = |val: &serde_json::Value| -> u32 {
        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    };
    if let Some(h) = cfg.hotkey.get("toggle") {
        let v = get_vk(h);
        if v != 0 {
            initial_vks.push(v);
        }
    }
    if let Some(h) = cfg.hotkey.get("mute") {
        let v = get_vk(h);
        if v != 0 {
            initial_vks.push(v);
        }
    }
    if let Some(h) = cfg.hotkey.get("unmute") {
        let v = get_vk(h);
        if v != 0 {
            initial_vks.push(v);
        }
    }
    if initial_vks.is_empty() {
        initial_vks.push(DEFAULT_HOTKEY_VK);
    }
    hotkey::HotkeyManager::new(initial_vks)
}

fn spawn_hotkey_loop(app_handle: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        // Hook installation moved to setup() for faster startup.
        // Wait briefly for the hook thread to be ready before entering the poll loop.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut topmost_counter: u32 = 0;
        let mut afk_counter: u32 = 0;
        let mut hook_maint_counter: u32 = 0;

        let extract_vk = |val: &serde_json::Value| -> u32 {
            val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
        };
        let refresh_hotkey_cache = |st: &config::AppConfig| -> (bool, u32, u32, u32) {
            let is_toggle = st.hotkey_mode == "toggle";
            let toggle_vk = st.hotkey.get("toggle").map(&extract_vk).unwrap_or(0);
            let mute_vk = st.hotkey.get("mute").map(&extract_vk).unwrap_or(0);
            let unmute_vk = st.hotkey.get("unmute").map(&extract_vk).unwrap_or(0);
            (is_toggle, toggle_vk, mute_vk, unmute_vk)
        };

        let st = state.config.lock();
        let hk_init = refresh_hotkey_cache(&st);
        let mut cached_is_toggle = hk_init.0;
        let mut cached_toggle_vk = hk_init.1;
        let mut cached_mute_vk = hk_init.2;
        let mut cached_unmute_vk = hk_init.3;
        let mut cached_afk_enabled = st.afk.enabled;
        let mut cached_afk_timeout = st.afk.timeout;
        #[allow(unused_assignments)]
        let mut cached_overlay_enabled = st.persistent_overlay.values().any(|o| o.enabled);
        drop(st);

        const AFK_CHECK_TICKS: u32 = 100;
        const HOOK_MAINT_TICKS: u32 = 500; // Every 5 seconds
        let topmost_ticks =
            (constants::OVERLAY_TOPMOST_INTERVAL_MS / constants::HOTKEY_POLL_INTERVAL_MS) as u32;
        let mut diag_counter: u32 = 0;

        loop {
            // Periodic diagnostic heartbeat (every ~5s)
            diag_counter += 1;
            if diag_counter >= 500 {
                diag_counter = 0;
                tracing::debug!(
                    toggle_vk = cached_toggle_vk,
                    is_toggle = cached_is_toggle,
                    "hotkey loop heartbeat"
                );
            }

            // ── Process hotkey events ──
            {
                let hk = state.hotkeys.lock();
                while let Some(vk) = hk.try_recv() {
                    tracing::info!(vk = vk, "Hotkey VK received from hook");
                    if cached_is_toggle {
                        if cached_toggle_vk == vk {
                            do_toggle_mute(&app_handle);
                        }
                    } else if cached_mute_vk == cached_unmute_vk && cached_mute_vk == vk {
                        do_toggle_mute(&app_handle);
                    } else if cached_mute_vk == vk {
                        do_set_mute(&app_handle, true);
                    } else if cached_unmute_vk == vk {
                        do_set_mute(&app_handle, false);
                    }
                }
            }

            // ── AFK Logic ──
            afk_counter += 1;
            if afk_counter >= AFK_CHECK_TICKS {
                afk_counter = 0;
                if cached_afk_enabled {
                    let idle_s = crate::utils::get_idle_duration();
                    if idle_s > (cached_afk_timeout as f32) {
                        let is_muted = *state.is_muted.lock();
                        if !is_muted {
                            do_set_mute(&app_handle, true);
                        }
                    }
                }
            }

            // ── Periodic maintenance ──
            topmost_counter += 1;
            if topmost_counter >= topmost_ticks {
                topmost_counter = 0;

                // Refresh cached config values
                {
                    let st = state.config.lock();
                    let new_cache = refresh_hotkey_cache(&st);
                    cached_is_toggle = new_cache.0;
                    cached_toggle_vk = new_cache.1;
                    cached_mute_vk = new_cache.2;
                    cached_unmute_vk = new_cache.3;
                    cached_afk_enabled = st.afk.enabled;
                    cached_afk_timeout = st.afk.timeout;
                    cached_overlay_enabled = st.persistent_overlay.values().any(|o| o.enabled);
                }

                // Re-assert always-on-top for all active overlay windows.
                // Dispatched to main thread to avoid blocking this loop with
                // synchronous SendMessage calls from SetWindowPos.
                if cached_overlay_enabled {
                    let ah = app_handle.clone();
                    let _ = app_handle.run_on_main_thread(move || {
                        for (label, win) in ah.webview_windows() {
                            if (label == "overlay" || label.starts_with("overlay-"))
                                && let Ok(tauri_hwnd) = win.hwnd() {
                                    use windows::Win32::Foundation::HWND;
                                    let hwnd = HWND(tauri_hwnd.0);
                                    crate::utils::force_topmost(hwnd);
                                }
                        }
                    });
                }
            }

            hook_maint_counter += 1;
            if hook_maint_counter >= HOOK_MAINT_TICKS {
                hook_maint_counter = 0;
                let hk = state.hotkeys.lock();
                hk.ensure_hook_active();
            }

            std::thread::sleep(std::time::Duration::from_millis(constants::HOTKEY_POLL_INTERVAL_MS));
        }
    });
}

// ─────────────────────────────────────────
//  App entry point
// ─────────────────────────────────────────
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown".to_string());
        let payload = info.payload().downcast_ref::<&str>().copied().or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str())).unwrap_or("no payload");
        tracing::error!(panic = %payload, location = %location, "APPLICATION PANIC");
        eprintln!("PANIC: {} at {}", payload, location);
    }));

    let log_path = {
        let mut p = std::env::temp_dir();
        p.push("micmute_debug.log");
        p
    };
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .ok();
    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .with_target(true)
            .with_writer(std::sync::Mutex::new(file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_target(true)
            .init();
    }

    tracing::info!("Starting MicMuteRs application");

    unsafe {
        tracing::debug!("Setting process priority");
        let _ = windows::Win32::System::Threading::SetPriorityClass(
            windows::Win32::System::Threading::GetCurrentProcess(),
            windows::Win32::System::Threading::ABOVE_NORMAL_PRIORITY_CLASS,
        );
    }

    tracing::debug!("Loading config");
    let mut cfg = config::AppConfig::load();
    tracing::debug!("Getting audio devices");
    // Run on a separate thread so COM (COINIT_MULTITHREADED) never contaminates
    // the main thread's apartment — tao requires OleInitialize (STA) on the main thread.
    let devices = std::thread::spawn(|| audio::get_audio_devices().unwrap_or_default())
        .join()
        .unwrap_or_default();
    tracing::debug!("Initializing hotkeys");
    let hotkey_mgr = init_hotkeys(&cfg);

    tracing::debug!("Creating audio channel");
    let (audio_tx, audio_rx) = std_mpsc::sync_channel::<AudioMsg>(10);

    let state = Arc::new(AppState {
        config: Mutex::new(cfg.clone()),
        hotkeys: Mutex::new(hotkey_mgr),
        is_muted: Mutex::new(false),
        peak_level: std::sync::atomic::AtomicU32::new(0),
        available_devices: Mutex::new(devices.clone()),
        audio_tx,
        tray: Mutex::new(None),
    });

    tracing::info!("Building Tauri application");
    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|win, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && matches!(win.label(), "settings" | "about") {
                    let _ = win.hide();
                    api.prevent_close();
                }
        })
        .setup({
            let state = Arc::clone(&state);
            let initial_device_id = cfg.device_id.clone();
            move |app| {
                tracing::info!("Tauri setup starting");
                // ── Audio worker thread ──
                spawn_audio_worker(app.handle().clone(), Arc::clone(&state), audio_rx, initial_device_id);

                // ── System tray ──
                tracing::debug!("Initializing system tray");
                let is_light = theme::is_system_light_theme();
                let tray_icon = load_tray_icon(false, is_light).ok();
                let tray_menu = build_tray_menu(app, &cfg, &devices).ok();

                let version = env!("CARGO_PKG_VERSION");
                let mut tray_builder = TrayIconBuilder::with_id("main")
                    .tooltip(&format!("MicMuteRs v{version} — Unmuted"));

                if let Some(icon) = tray_icon {
                    tray_builder = tray_builder.icon(icon);
                }
                if let Some(menu) = tray_menu {
                    tray_builder = tray_builder.menu(&menu);
                }

                let tray = tray_builder.build(app)?;

                *state.tray.lock() = Some(tray);

                // Register global event handlers on the App — builder-level callbacks
                // are not reliably invoked in Tauri 2.10.x/tao 0.34.x on Windows.
                {
                    let state2 = Arc::clone(&state);
                    app.on_menu_event(move |app, event| {
                        tracing::info!(id = event.id().as_ref(), "Tray menu event fired");
                        handle_tray_event(app, event.id().as_ref(), &state2);
                    });
                }
                app.on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        tracing::info!("Tray left-click: toggling mute");
                        let app = tray.app_handle();
                        do_toggle_mute(app);
                    }
                });

                // ── Hotkey listener thread ──
                tracing::debug!("Spawning hotkey loop");
                spawn_hotkey_loop(app.handle().clone(), Arc::clone(&state));

                // ── Configure static overlay & OSD windows ──
                // Uses index-based mapping: primary monitor → overlay/osd,
                // second monitor → overlay-2/osd-2.
                {
                    let monitors = get_monitor_info(app.handle());

                    // Ensure every connected monitor has a config entry.
                    // Missing monitors inherit from "primary" so the overlay/OSD
                    // starts at the correct size without waiting for a manual sync.
                    let mut cfg_dirty = false;
                    for mon in &monitors {
                        let key = &mon.label_key;
                        if !cfg.persistent_overlay.contains_key(key) {
                            let base = cfg.persistent_overlay.get("primary").cloned().unwrap_or_default();
                            cfg.persistent_overlay.insert(key.clone(), base);
                            cfg_dirty = true;
                            tracing::info!(monitor = %key, "Created overlay config entry from primary");
                        }
                        if !cfg.osd.contains_key(key) {
                            let base = cfg.osd.get("primary").cloned().unwrap_or_default();
                            cfg.osd.insert(key.clone(), base);
                            cfg_dirty = true;
                            tracing::info!(monitor = %key, "Created OSD config entry from primary");
                        }
                    }
                    if cfg_dirty {
                        *state.config.lock() = cfg.clone();
                        if let Err(e) = cfg.save() {
                            tracing::error!(error = %e, "Failed to save auto-populated monitor configs");
                        }
                    }

                    sync_overlay_windows(app.handle(), &cfg, &monitors);
                    sync_osd_windows(app.handle(), &cfg, &monitors);
                    tracing::info!(
                        monitor_count = monitors.len(),
                        "Static overlay/OSD windows configured"
                    );

                    // Emit config-update so overlay/OSD JS re-queries monitor keys
                    // and applies correct per-monitor sizing after the window map is populated.
                    let _ = app.emit("config-update", serde_json::json!({ "config": cfg }));

                    // Safety net: re-emit after 500ms for overlay windows whose JS
                    // hasn't registered listeners yet. The handler is idempotent.
                    {
                        let app_handle = app.handle().clone();
                        let cfg_clone = cfg.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            let _ = app_handle.emit("config-update", serde_json::json!({ "config": cfg_clone }));
                        });
                    }
                }

                // ── Install keyboard hook ──
                // Now that WebView2 windows are initialized, install the LL
                // keyboard hook on a short delay so it lands last in the
                // Windows hook chain (LIFO — last registered = first called).
                {
                    let state_hk = Arc::clone(&state);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let hk = state_hk.hotkeys.lock();
                        hk.start_hook();
                        tracing::info!("Keyboard hook installed");
                    });
                }

                tracing::info!("Tauri setup complete");
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::toggle_mute,
            commands::set_mute,
            commands::get_config,
            commands::update_config,
            commands::get_cached_devices,
            commands::get_devices,
            commands::set_device,
            commands::start_recording_hotkey,
            commands::stop_recording_hotkey,
            commands::get_recorded_hotkey,
            commands::set_run_on_startup_cmd,
            commands::get_run_on_startup_cmd,
            commands::open_url,
            commands::get_app_version,
            commands::pick_audio_file,
            commands::preview_audio_feedback,
            commands::get_overlay_background_is_light,
            commands::save_overlay_position,
            commands::get_overlay_topmost_interval,
            commands::get_monitors,
            commands::get_window_monitor_key,
        ])
        .run(tauri::generate_context!())
        .expect("fatal error while running tauri application");
}

// ─────────────────────────────────────────
//  Action helpers
// ─────────────────────────────────────────
pub fn do_toggle_mute(app: &AppHandle) {
    tracing::info!("do_toggle_mute called");
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().clone();
    if let Err(e) = state.audio_tx.try_send(AudioMsg::ToggleMute(cfg)) {
        tracing::error!(error = ?e, "do_toggle_mute: failed to send to audio worker");
    }
}

pub fn do_set_mute(app: &AppHandle, mute: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().clone();
    let _ = state.audio_tx.try_send(AudioMsg::SetMute(mute, cfg));
}

pub fn update_tray_icon(app: &AppHandle, is_muted: bool) {
    let is_light = theme::is_system_light_theme();
    if let Ok(icon) = load_tray_icon(is_muted, is_light)
        && let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_icon(Some(icon));
            let version = env!("CARGO_PKG_VERSION");
            let state = if is_muted { "Muted" } else { "Unmuted" };
            let _ = tray.set_tooltip(Some(&format!("MicMuteRs v{version} — {state}")));
        }
}

// ─────────────────────────────────────────
//  OSD trigger (multi-monitor)
// ─────────────────────────────────────────
pub fn trigger_osd(app: &AppHandle, is_muted: bool, cfg: &config::AppConfig, monitors: &[MonitorInfo]) {
    // Sort monitors: primary first (same order as sync_osd_windows).
    let mut sorted: Vec<&MonitorInfo> = monitors.iter().collect();
    sorted.sort_by_key(|m| !m.is_primary);

    for (idx, mon) in sorted.iter().enumerate() {
        if idx >= OSD_LABELS.len() {
            break;
        }
        let label = OSD_LABELS[idx];
        let key = &mon.label_key;
        let osd_cfg = match cfg.osd.get(key) {
            Some(c) if c.enabled => c,
            _ => continue,
        };

        let duration = osd_cfg.duration;
        let size = osd_cfg.size;
        let opacity = osd_cfg.opacity;
        let position = osd_cfg.position.clone();
        let theme = osd_cfg.theme.clone();

        let osd_win = app.get_webview_window(label);

        if let Some(osd_win) = osd_win {
            let _ = osd_win.set_size(tauri::LogicalSize::new(size as f64, size as f64));

            // Position on the designated monitor
            let mon_pos = mon.position;
            let mon_size = mon.size;
            let scale = mon.scale_factor;

            let mon_w = mon_size.width as f64 / scale;
            let mon_h = mon_size.height as f64 / scale;
            let w = size as f64;
            let h = size as f64;
            let x = (mon_w - w) / 2.0;
            let y = match position.as_str() {
                "Top" => 50.0,
                "Bottom" | "Bottom-Center" => mon_h - h - 100.0,
                _ => (mon_h - h) / 2.0,
            };
            let _ = osd_win.set_position(tauri::PhysicalPosition::new(
                mon_pos.x + (x * scale) as i32,
                mon_pos.y + (y * scale) as i32,
            ));

            let _ = osd_win.show();
            let _ = osd_win.set_always_on_top(true);
            let _ = osd_win.emit(
                "osd-show",
                serde_json::json!({
                    "is_muted": is_muted,
                    "duration": duration,
                    "opacity": opacity,
                    "theme": theme,
                    "is_system_light": theme::is_system_light_theme(),
                }),
            );

            // Schedule hide via per-monitor timer.
            let label_owned = label.to_string();
            let mut timers = get_osd_timers().lock();
            let timer = timers.entry(label_owned).or_insert_with(OsdTimer::new);
            timer.schedule_hide(osd_win, std::time::Duration::from_millis(duration as u64));
        }
    }
}

// ─────────────────────────────────────────
//  Tray menu event handler
// ─────────────────────────────────────────
fn handle_tray_event(app: &AppHandle, id: &str, state: &Arc<AppState>) {
    match id {
        "quit" => {
            app.exit(0);
        }
        "toggle_mute" => {
            do_toggle_mute(app);
        }
        "toggle_sound" => {
            let cfg = {
                let mut cfg = state.config.lock();
                cfg.beep_enabled = !cfg.beep_enabled;
                cfg.clone()
            };
            if let Err(e) = cfg.save() {
                tracing::error!("{}", e);
            }
            sync_tray_and_emit(app, state, &cfg);
        }
        "toggle_osd" => {
            let cfg = {
                let mut cfg = state.config.lock();
                let any_enabled = cfg.osd.values().any(|o| o.enabled);
                for osd_cfg in cfg.osd.values_mut() {
                    osd_cfg.enabled = !any_enabled;
                }
                cfg.clone()
            };
            if let Err(e) = cfg.save() {
                tracing::error!("{}", e);
            }
            // OSD windows are shown on-demand; no sync needed here
            sync_tray_and_emit(app, state, &cfg);
        }
        "toggle_overlay" => {
            let cfg = {
                let mut cfg = state.config.lock();
                let any_enabled = cfg.persistent_overlay.values().any(|o| o.enabled);
                for overlay_cfg in cfg.persistent_overlay.values_mut() {
                    overlay_cfg.enabled = !any_enabled;
                }
                cfg.clone()
            };
            if let Err(e) = cfg.save() {
                tracing::error!("{}", e);
            }
            // Show/hide all static overlay windows
            let monitors = get_monitor_info(app);
            sync_overlay_windows(app, &cfg, &monitors);
            sync_tray_and_emit(app, state, &cfg);
        }

        "toggle_boot" => {
            let current = startup::get_run_on_startup();
            startup::set_run_on_startup(!current);
            let actual = startup::get_run_on_startup();
            let cfg = state.config.lock().clone();
            sync_tray_and_emit(app, state, &cfg);
            // Notify settings UI so its checkbox stays in sync
            let _ = app.emit("startup-changed", serde_json::json!({ "enabled": actual }));
        }
        "settings" => {
            tracing::info!("Settings menu item clicked");
            if let Some(win) = app.get_webview_window("settings") {
                tracing::info!("Showing settings window");
                if let Err(e) = win.show() {
                    tracing::error!(error = ?e, "Failed to show settings window");
                }
                let _ = win.set_focus();
            } else {
                tracing::error!("Settings window not found");
            }
        }
        "help" => {
            let _ = open::that("https://github.com/madbeat14/MicMute-RS-Tauri");
        }
        "about" => {
            if let Some(win) = app.get_webview_window("about") {
                let _ = win.show();
                let _ = win.center();
                let _ = win.set_focus();
            }
        }
        id if id.starts_with("mic_") => {
            let dev_id = &id[4..];
            let new_device_id = if dev_id == "default" {
                None
            } else {
                Some(dev_id.to_string())
            };
            let cfg = {
                let mut cfg = state.config.lock();
                cfg.device_id = new_device_id.clone();
                cfg.clone()
            };
            if let Err(e) = cfg.save() {
                tracing::error!("{}", e);
            }
            let _ = state.audio_tx.try_send(AudioMsg::SetDevice(new_device_id));
            sync_tray_and_emit(app, state, &cfg);
        }
        _ => {}
    }
}

fn sync_tray_and_emit(app: &AppHandle, state: &Arc<AppState>, cfg: &config::AppConfig) {
    if let Some(tray) = app.tray_by_id("main") {
        let devices = state.available_devices.lock().clone();
        if let Ok(menu) = build_tray_menu(app, cfg, &devices) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    let _ = app.emit("config-update", serde_json::json!({ "config": cfg }));
}
