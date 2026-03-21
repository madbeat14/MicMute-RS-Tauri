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
use std::sync::{Arc, Mutex};
use std::sync::mpsc as std_mpsc;
use tracing;
use tauri::{
    AppHandle, Emitter, Manager,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Message sent to the audio feedback worker thread.
struct AudioFeedbackMsg {
    stream_handle: rodio::OutputStreamHandle,
    is_muted: bool,
    config: config::AppConfig,
}

// ─────────────────────────────────────────
//  Shared application state
// ─────────────────────────────────────────
pub struct AppState {
    pub audio: Mutex<audio::AudioController>,
    pub config: Mutex<config::AppConfig>,
    pub hotkeys: Mutex<hotkey::HotkeyManager>,
    pub is_muted: Mutex<bool>,
    pub available_devices: Mutex<Vec<(String, String)>>,
    /// Channel to send audio feedback work to a single persistent worker thread,
    /// avoiding per-toggle thread spawns (~1MB stack each).
    audio_feedback_tx: std_mpsc::Sender<AudioFeedbackMsg>,
}

/// # Safety Invariants
///
/// 1. All COM interfaces are accessed only from the main thread (STA)
/// 2. `OutputStream` is created on the main thread and never moved
/// 3. The `rodio` `OutputStream` is not Send, but we ensure it's only
///    accessed through the Mutex on the thread that created it
/// 4. All Windows messages are processed on the main thread
///
/// Violating these invariants could lead to COM threading errors or
/// audio playback issues.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

// ─────────────────────────────────────────
//  Tray helpers
// ─────────────────────────────────────────
pub(crate) fn build_tray_menu<M: Manager<tauri::Wry>>(
    app: &M,
    cfg: &config::AppConfig,
    devices: &[(String, String)],
) -> Menu<tauri::Wry> {
    let menu = Menu::new(app).unwrap();

    let toggle_item =
        MenuItem::with_id(app, "toggle_mute", "Toggle Mute", true, None::<&str>).unwrap();
    let _ = menu.append(&toggle_item);
    let _ = menu.append(&PredefinedMenuItem::separator(app).unwrap());

    // Microphone submenu
    let mic_menu = Submenu::new(app, "Select Microphone", true).unwrap();
    let default_item = CheckMenuItem::with_id(
        app,
        "mic_default",
        "Default Windows Device",
        true,
        cfg.device_id.is_none(),
        None::<&str>,
    )
    .unwrap();
    let _ = mic_menu.append(&default_item);
    for (id, name) in devices {
        let is_sel = cfg.device_id.as_ref() == Some(id);
        let key = format!("mic_{}", id);
        let item = CheckMenuItem::with_id(app, key, name, true, is_sel, None::<&str>).unwrap();
        let _ = mic_menu.append(&item);
    }
    let _ = menu.append(&mic_menu);

    let _ = menu.append(&PredefinedMenuItem::separator(app).unwrap());

    let sound_item = CheckMenuItem::with_id(
        app,
        "toggle_sound",
        "Play Sound on Toggle",
        true,
        cfg.beep_enabled,
        None::<&str>,
    )
    .unwrap();
    let osd_item = CheckMenuItem::with_id(
        app,
        "toggle_osd",
        "Enable OSD Notification",
        true,
        cfg.osd.enabled,
        None::<&str>,
    )
    .unwrap();
    let overlay_item = CheckMenuItem::with_id(
        app,
        "toggle_overlay",
        "Show Persistent Overlay",
        true,
        cfg.persistent_overlay.enabled,
        None::<&str>,
    )
    .unwrap();
    let boot_item = CheckMenuItem::with_id(
        app,
        "toggle_boot",
        "Start on Boot",
        true,
        startup::get_run_on_startup(),
        None::<&str>,
    )
    .unwrap();

    let _ = menu.append_items(&[
        &sound_item,
        &osd_item,
        &overlay_item,
        &boot_item,
        &PredefinedMenuItem::separator(app).unwrap(),
        &MenuItem::with_id(app, "settings", "Settings", true, None::<&str>).unwrap(),
        &MenuItem::with_id(app, "help", "Help", true, None::<&str>).unwrap(),
        &PredefinedMenuItem::separator(app).unwrap(),
        &MenuItem::with_id(app, "quit", "Exit", true, None::<&str>).unwrap(),
    ]);

    menu
}

pub fn load_tray_icon(is_muted: bool, is_light: bool) -> Image<'static> {
    let bytes: &[u8] = match (is_muted, is_light) {
        (true, true) => include_bytes!("../frontend/assets/mic_muted_black.ico"),
        (false, true) => include_bytes!("../frontend/assets/mic_black.ico"),
        (true, false) => include_bytes!("../frontend/assets/mic_muted_white.ico"),
        (false, false) => include_bytes!("../frontend/assets/mic_white.ico"),
    };
    // SAFETY: include_bytes! is compile-time verified, so this should never fail.
    // Using unwrap() here because a failure indicates a build/packaging issue.
    Image::from_bytes(bytes).expect("included tray icon bytes should always be valid")
}

// ─────────────────────────────────────────
//  Emit helper – fires state update to all windows
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

// ─────────────────────────────────────────
//  App entry point
// ─────────────────────────────────────────
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Initialize logging ──
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .with_target(true)
        .init();

    tracing::info!("Starting MicMuteRs application");

    // ── Pre-initialize state BEFORE Tauri builder to prevent IPC race conditions ──
    unsafe {
        let _ = windows::Win32::System::Threading::SetPriorityClass(
            windows::Win32::System::Threading::GetCurrentProcess(),
            windows::Win32::System::Threading::ABOVE_NORMAL_PRIORITY_CLASS,
        );
    }

    let cfg = config::AppConfig::load();
    let audio_ctrl = match audio::AudioController::new(cfg.device_id.as_ref())
        .or_else(|e| {
            tracing::error!(error = ?e, "Failed to initialize configured audio device, falling back to default");
            audio::AudioController::new(None)
        }) {
        Ok(ctrl) => ctrl,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to initialize any audio controller");
            std::process::exit(1);
        }
    };

    let is_muted = audio_ctrl.is_muted().unwrap_or(false);
    let devices = audio::get_audio_devices().unwrap_or_default();

    // ── Hotkeys ──
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
    let hotkey_mgr = hotkey::HotkeyManager::new(initial_vks);

    // ── Audio feedback worker thread (replaces per-toggle thread spawns) ──
    let (audio_tx, audio_rx) = std_mpsc::channel::<AudioFeedbackMsg>();
    std::thread::Builder::new()
        .name("audio-feedback".into())
        .stack_size(256 * 1024) // 256KB is plenty for audio playback
        .spawn(move || {
            // Holds the currently-playing Sink. When a new message arrives,
            // this gets replaced — dropping the old Sink stops its playback
            // immediately, so rapid toggles never queue up.
            let mut _active_sink: Option<rodio::Sink> = None;
            for msg in audio_rx {
                _active_sink = audio::play_feedback(&msg.stream_handle, msg.is_muted, &msg.config);
            }
        })
        .expect("failed to spawn audio feedback worker");

    // ── Shared state ──
    let state = Arc::new(AppState {
        audio: Mutex::new(audio_ctrl),
        config: Mutex::new(cfg.clone()),
        hotkeys: Mutex::new(hotkey_mgr),
        is_muted: Mutex::new(is_muted),
        available_devices: Mutex::new(devices.clone()),
        audio_feedback_tx: audio_tx,
    });

    tauri::Builder::default()
        .manage(Arc::clone(&state))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|win, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if win.label() == "settings" {
                    let _ = win.hide();
                    api.prevent_close();
                }
            }
            _ => {}
        })
        .setup(move |app| {
            // ── System tray ──
            let is_light = theme::is_system_light_theme();
            let tray_icon = load_tray_icon(is_muted, is_light);
            let tray_menu = build_tray_menu(app, &cfg, &devices);

            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("MicMuteRs")
                .menu(&tray_menu)
                .on_menu_event({
                    let state2 = Arc::clone(&state);
                    move |app, event| {
                        handle_tray_event(app, event.id().as_ref(), &state2);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        do_toggle_mute(&app);
                    }
                })
                .build(app)?;

            // ── Overlay window ──
            let overlay_win = app.get_webview_window("overlay").unwrap();
            {
                let cfg_guard = state.config.lock().unwrap();
                if cfg_guard.persistent_overlay.enabled {
                    // Stored x/y are physical pixels (from outerPosition() in JS)
                    let _ = overlay_win.set_position(tauri::PhysicalPosition::new(
                        cfg_guard.persistent_overlay.x,
                        cfg_guard.persistent_overlay.y,
                    ));
                    let scale = cfg_guard.persistent_overlay.scale as f64;
                    let w = if cfg_guard.persistent_overlay.show_vu {
                        scale + 30.0
                    } else {
                        scale
                    };
                    let _ = overlay_win.set_size(tauri::LogicalSize::new(w, scale));
                    // Bootstrap transparency: Tauri's set_ignore_cursor_events(true)
                    // triggers TAO to properly add WS_EX_LAYERED (required for WebView2
                    // per-pixel alpha). We call it once, then use our safe
                    // set_click_through() to set the actual desired click-through state
                    // without ever rebuilding the full extended style.
                    let _ = overlay_win.set_ignore_cursor_events(true);
                    if !cfg_guard.persistent_overlay.locked {
                        if let Ok(tauri_hwnd) = overlay_win.hwnd() {
                            use windows::Win32::Foundation::HWND;
                            let hwnd = HWND(tauri_hwnd.0);
                            crate::utils::set_click_through(hwnd, false);
                        }
                    }
                    let _ = overlay_win.show();
                    // Explicitly re-assert always-on-top after show to ensure
                    // Windows applies the TOPMOST z-order (the config flag alone
                    // can be lost on system boot when other apps start later).
                    let _ = overlay_win.set_always_on_top(true);
                }
            }

            // ── Hotkey listener thread ──
            let app_handle = app.handle().clone();
            let state_for_thread = Arc::clone(&state);
            std::thread::spawn(move || {
                // Wait for a few seconds to let WebView2 load its own hooks, making our hook LAST in the chain
                std::thread::sleep(std::time::Duration::from_secs(2));
                {
                    let hk = state_for_thread.hotkeys.lock().unwrap();
                    hk.start_hook();
                }

                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(
                        None,
                        windows::Win32::System::Com::COINIT_MULTITHREADED,
                    );
                }
                let state = state_for_thread;
                let mut topmost_counter: u32 = 0;
                let mut afk_counter: u32 = 0;

                // Cache hotkey config as plain values to avoid cloning HashMap every iteration.
                // These are refreshed from config periodically (every ~500ms).
                let extract_vk = |val: &serde_json::Value| -> u32 {
                    val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                };
                let refresh_hotkey_cache = |st: &config::AppConfig| -> (bool, u32, u32, u32) {
                    let is_toggle = st.hotkey_mode == "toggle";
                    let toggle_vk = st.hotkey.get("toggle").map(|v| extract_vk(v)).unwrap_or(0);
                    let mute_vk = st.hotkey.get("mute").map(|v| extract_vk(v)).unwrap_or(0);
                    let unmute_vk = st.hotkey.get("unmute").map(|v| extract_vk(v)).unwrap_or(0);
                    (is_toggle, toggle_vk, mute_vk, unmute_vk)
                };

                let st = state.config.lock().unwrap();
                let hk_init = refresh_hotkey_cache(&st);
                let mut cached_is_toggle = hk_init.0;
                let mut cached_toggle_vk = hk_init.1;
                let mut cached_mute_vk = hk_init.2;
                let mut cached_unmute_vk = hk_init.3;
                let mut cached_afk_enabled = st.afk.enabled;
                let mut cached_afk_timeout = st.afk.timeout;
                #[allow(unused_assignments)]
                let mut cached_overlay_enabled = st.persistent_overlay.enabled;
                drop(st);

                // AFK check interval (~1 second at 10ms polling)
                const AFK_CHECK_TICKS: u32 = 100;
                let topmost_ticks = (constants::OVERLAY_TOPMOST_INTERVAL_MS / constants::HOTKEY_POLL_INTERVAL_MS) as u32;

                loop {
                    // ── Process hotkey events (no config lock needed) ──
                    {
                        let hk = state.hotkeys.lock().unwrap();
                        while let Some(vk) = hk.try_recv() {
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

                    // ── AFK Logic (check every ~1s, not every 10ms) ──
                    afk_counter += 1;
                    if afk_counter >= AFK_CHECK_TICKS {
                        afk_counter = 0;
                        if cached_afk_enabled {
                            let mut lii =
                                windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO {
                                    cbSize: std::mem::size_of::<
                                        windows::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO,
                                    >() as u32,
                                    dwTime: 0,
                                };
                            let _ = unsafe {
                                windows::Win32::UI::Input::KeyboardAndMouse::GetLastInputInfo(
                                    &mut lii,
                                )
                            };
                            let tick = unsafe {
                                windows::Win32::System::SystemInformation::GetTickCount()
                            };
                            let elapsed_ms = tick.saturating_sub(lii.dwTime);

                            if elapsed_ms > (cached_afk_timeout * 1000) {
                                let is_muted = *state.is_muted.lock().unwrap();
                                if !is_muted {
                                    do_set_mute(&app_handle, true);
                                }
                            }
                        }
                    }

                    // ── Periodic maintenance (~every 500ms) ──
                    topmost_counter += 1;
                    if topmost_counter >= topmost_ticks {
                        topmost_counter = 0;

                        // Reinstall keyboard hook if Windows silently removed it
                        // (common during tray context menu modal loops)
                        {
                            let hk = state.hotkeys.lock().unwrap();
                            hk.ensure_hook_active();
                        }

                        // Refresh cached config values
                        {
                            let st = state.config.lock().unwrap();
                            let new_cache = refresh_hotkey_cache(&st);
                            cached_is_toggle = new_cache.0;
                            cached_toggle_vk = new_cache.1;
                            cached_mute_vk = new_cache.2;
                            cached_unmute_vk = new_cache.3;
                            cached_afk_enabled = st.afk.enabled;
                            cached_afk_timeout = st.afk.timeout;
                            cached_overlay_enabled = st.persistent_overlay.enabled;
                        }

                        if cached_overlay_enabled {
                            if let Some(win) = app_handle.get_webview_window("overlay") {
                                if let Ok(tauri_hwnd) = win.hwnd() {
                                    use windows::Win32::Foundation::HWND;
                                    let hwnd = HWND(tauri_hwnd.0);
                                    crate::utils::force_topmost(hwnd);
                                }
                            }
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            });

            Ok(())
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
            commands::pick_audio_file,
            commands::preview_audio_feedback,
            commands::get_overlay_background_is_light,
            commands::save_overlay_position,
            commands::get_overlay_topmost_interval,
        ])
        .run(tauri::generate_context!())
        .expect("fatal error while running tauri application");
}

// ─────────────────────────────────────────
//  Action helpers (called from tray + hotkey thread)
// ─────────────────────────────────────────
pub fn do_toggle_mute(app: &AppHandle) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap().clone();

    let (success_m, peak, stream_handle) = {
        let audio = state.audio.lock().unwrap();
        if let Ok(m) = audio.toggle_mute(&cfg) {
            let p = audio.get_peak_value().unwrap_or(0.0);
            (Some(m), p, Some(audio.stream_handle()))
        } else {
            (None, 0.0, None)
        }
    };

    if let Some(m) = success_m {
        *state.is_muted.lock().unwrap() = m;
        update_tray_icon(app, m);
        emit_state(app, m, peak);
        trigger_osd(app, m);

        if let Some(sh) = stream_handle {
            let _ = state.audio_feedback_tx.send(AudioFeedbackMsg {
                stream_handle: sh,
                is_muted: m,
                config: cfg,
            });
        }
    }
}

pub fn do_set_mute(app: &AppHandle, mute: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap().clone();

    let (success, peak, stream_handle) = {
        let audio = state.audio.lock().unwrap();
        if audio.set_mute(mute, &cfg).is_ok() {
            let p = audio.get_peak_value().unwrap_or(0.0);
            (true, p, Some(audio.stream_handle()))
        } else {
            (false, 0.0, None)
        }
    };

    if success {
        *state.is_muted.lock().unwrap() = mute;
        update_tray_icon(app, mute);
        emit_state(app, mute, peak);
        trigger_osd(app, mute);

        if let Some(sh) = stream_handle {
            let _ = state.audio_feedback_tx.send(AudioFeedbackMsg {
                stream_handle: sh,
                is_muted: mute,
                config: cfg,
            });
        }
    }
}

pub fn update_tray_icon(app: &AppHandle, is_muted: bool) {
    let is_light = theme::is_system_light_theme();
    let icon = load_tray_icon(is_muted, is_light);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(icon));
    }
}

/// Tracks the latest OSD hide generation to cancel stale hides.
static OSD_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn trigger_osd(app: &AppHandle, is_muted: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap();
    if !cfg.osd.enabled {
        return;
    }
    let duration = cfg.osd.duration;
    let size = cfg.osd.size;
    let opacity = cfg.osd.opacity;
    let position = cfg.osd.position.clone();
    drop(cfg);

    if let Some(osd_win) = app.get_webview_window("osd") {
        // Resize to configured size
        let _ = osd_win.set_size(tauri::LogicalSize::new(size as f64, size as f64));
        // Position based on config
        if let Ok(Some(monitor)) = osd_win.current_monitor() {
            let mon_size = monitor.size();
            let scale = monitor.scale_factor();
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
                (x * scale) as i32,
                (y * scale) as i32,
            ));
        }
        let _ = osd_win.show();
        let _ = osd_win.set_always_on_top(true);
        let _ = osd_win.emit(
            "osd-show",
            serde_json::json!({ "is_muted": is_muted, "duration": duration, "opacity": opacity }),
        );

        // Bump generation and schedule hide; stale generations are ignored.
        let generation = OSD_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let win_clone = osd_win.clone();
        let dur = std::time::Duration::from_millis(duration as u64);
        std::thread::spawn(move || {
            std::thread::sleep(dur);
            // Only hide if no newer OSD was triggered in the meantime
            if OSD_GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation {
                let _ = win_clone.hide();
            }
        });
    }
}

// ─────────────────────────────────────────
//  Tray menu event handler
// ─────────────────────────────────────────
fn handle_tray_event(app: &AppHandle, id: &str, state: &Arc<AppState>) {
    match id {
        "quit" => {
            std::process::exit(0);
        }
        "toggle_mute" => {
            do_toggle_mute(app);
        }
        "toggle_sound" => {
            let cfg = {
                let mut cfg = state.config.lock().unwrap();
                cfg.beep_enabled = !cfg.beep_enabled;
                cfg.save();
                cfg.clone()
            };
            sync_tray_and_emit(app, state, &cfg);
        }
        "toggle_osd" => {
            let cfg = {
                let mut cfg = state.config.lock().unwrap();
                cfg.osd.enabled = !cfg.osd.enabled;
                cfg.save();
                cfg.clone()
            };
            sync_tray_and_emit(app, state, &cfg);
        }
        "toggle_overlay" => {
            let (enabled, cfg) = {
                let mut cfg = state.config.lock().unwrap();
                cfg.persistent_overlay.enabled = !cfg.persistent_overlay.enabled;
                cfg.save();
                (cfg.persistent_overlay.enabled, cfg.clone())
            };
            if let Some(win) = app.get_webview_window("overlay") {
                if enabled {
                    let _ = win.show();
                    let _ = win.set_always_on_top(true);
                } else {
                    let _ = win.hide();
                }
            }
            sync_tray_and_emit(app, state, &cfg);
        }
        "toggle_boot" => {
            let current = startup::get_run_on_startup();
            startup::set_run_on_startup(!current);
            // Rebuild tray to update the checkmark
            let cfg = state.config.lock().unwrap().clone();
            sync_tray_and_emit(app, state, &cfg);
        }
        "settings" => {
            if let Some(win) = app.get_webview_window("settings") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
        "help" => {
            let _ = open::that("https://github.com/madbeat14/MicMuteRS");
        }
        id if id.starts_with("mic_") => {
            let dev_id = &id[4..];
            let new_device_id = if dev_id == "default" {
                None
            } else {
                Some(dev_id.to_string())
            };
            let cfg = {
                let mut cfg = state.config.lock().unwrap();
                cfg.device_id = new_device_id.clone();
                cfg.save();
                cfg.clone()
            };
            if let Ok(new_audio) = audio::AudioController::new(new_device_id.as_ref()) {
                *state.audio.lock().unwrap() = new_audio;
            }
            sync_tray_and_emit(app, state, &cfg);
        }
        _ => {}
    }
}

/// Rebuild tray menu checkmarks and emit config-update to all frontend windows.
fn sync_tray_and_emit(app: &AppHandle, state: &Arc<AppState>, cfg: &config::AppConfig) {
    if let Some(tray) = app.tray_by_id("main") {
        let devices = state.available_devices.lock().unwrap().clone();
        let menu = build_tray_menu(app, cfg, &devices);
        let _ = tray.set_menu(Some(menu));
    }
    let _ = app.emit(
        "config-update",
        serde_json::json!({ "config": cfg }),
    );
}
