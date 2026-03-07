pub mod audio;
pub mod com_interfaces;
pub mod commands;
pub mod config;
pub mod hotkey;
pub mod startup;
pub mod utils;

use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, Manager,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

// ─────────────────────────────────────────
//  Shared application state
// ─────────────────────────────────────────
pub struct AppState {
    pub audio: Mutex<audio::AudioController>,
    pub config: Mutex<config::AppConfig>,
    pub hotkeys: Mutex<hotkey::HotkeyManager>,
    pub is_muted: Mutex<bool>,
    pub available_devices: Mutex<Vec<(String, String)>>,
}

// SAFETY: All mutable access is serialized through Mutex.
// Windows COM interfaces (IMMDevice, etc.) and rodio OutputStream
// are not auto-Send, but we only ever access them behind a Mutex
// from a single Windows process, so this is safe in practice.
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
        (true, true) => include_bytes!("../assets/mic_muted_black.png"),
        (false, true) => include_bytes!("../assets/mic_black.png"),
        (true, false) => include_bytes!("../assets/mic_muted_white.png"),
        (false, false) => include_bytes!("../assets/mic_white.png"),
    };
    Image::from_bytes(bytes).expect("failed to load tray icon")
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
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|win, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if win.label() == "settings" {
                    let _ = win.hide();
                    api.prevent_close();
                }
            }
            _ => {}
        })
        .setup(|app| {
            // ── Load config & audio ──
            let cfg = config::AppConfig::load();
            let audio_ctrl = audio::AudioController::new(cfg.device_id.as_ref())
                .or_else(|_| audio::AudioController::new(None))
                .expect("Failed to initialize audio controller");

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
                initial_vks.push(0xB3);
            }
            let hotkey_mgr = hotkey::HotkeyManager::new(initial_vks);

            // ── Shared state ──
            let state = Arc::new(AppState {
                audio: Mutex::new(audio_ctrl),
                config: Mutex::new(cfg.clone()),
                hotkeys: Mutex::new(hotkey_mgr),
                is_muted: Mutex::new(is_muted),
                available_devices: Mutex::new(devices.clone()),
            });
            app.manage(Arc::clone(&state));

            // ── System tray ──
            let is_light = utils::is_system_light_theme();
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
                    let _ = overlay_win.set_position(tauri::LogicalPosition::new(
                        cfg_guard.persistent_overlay.x as f64,
                        cfg_guard.persistent_overlay.y as f64,
                    ));
                    let scale = cfg_guard.persistent_overlay.scale as f64;
                    let w = if cfg_guard.persistent_overlay.show_vu {
                        scale + 30.0
                    } else {
                        scale
                    };
                    let _ = overlay_win.set_size(tauri::LogicalSize::new(w, scale));
                    let _ =
                        overlay_win.set_ignore_cursor_events(cfg_guard.persistent_overlay.locked);
                    let _ = overlay_win.show();
                }
            }

            // ── Hotkey listener thread ──
            let app_handle = app.handle().clone();
            let state_for_thread = Arc::clone(&state);
            std::thread::spawn(move || {
                let state = state_for_thread;
                loop {
                    {
                        let st = state.config.lock().unwrap();
                        let mode = st.hotkey_mode.clone();
                        let hotkey_cfg = st.hotkey.clone();
                        drop(st);

                        let hk = state.hotkeys.lock().unwrap();
                        if let Some(vk) = hk.try_recv() {
                            drop(hk);
                            let get_vk = |val: &serde_json::Value| -> u32 {
                                val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                            };
                            if mode == "toggle" {
                                if hotkey_cfg.get("toggle").map(|v| get_vk(v)).unwrap_or(0) == vk {
                                    do_toggle_mute(&app_handle);
                                }
                            } else {
                                if hotkey_cfg.get("mute").map(|v| get_vk(v)).unwrap_or(0) == vk {
                                    do_set_mute(&app_handle, true);
                                } else if hotkey_cfg.get("unmute").map(|v| get_vk(v)).unwrap_or(0)
                                    == vk
                                {
                                    do_set_mute(&app_handle, false);
                                }
                            }
                        } else {
                            drop(hk);
                        }
                    }

                    // ── AFK Logic ──
                    {
                        let st = state.config.lock().unwrap();
                        if st.afk.enabled {
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

                            if elapsed_ms > (st.afk.timeout * 1000) {
                                let is_muted = *state.is_muted.lock().unwrap();
                                if !is_muted {
                                    drop(st); // drop lock before mutation
                                    do_set_mute(&app_handle, true);
                                }
                            }
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            });

            // ── Emit initial config to frontend ──
            let cfg_json = {
                let cfg_guard = state.config.lock().unwrap();
                serde_json::to_value(&*cfg_guard).unwrap_or_default()
            };
            let devs_json: Vec<serde_json::Value> = devices
                .iter()
                .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
                .collect();
            let _ = app.emit(
                "initial-data",
                serde_json::json!({
                    "config": cfg_json,
                    "devices": devs_json,
                    "is_muted": is_muted,
                    "peak_level": 0.0_f32,
                }),
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::toggle_mute,
            commands::set_mute,
            commands::get_config,
            commands::update_config,
            commands::get_devices,
            commands::set_device,
            commands::start_recording_hotkey,
            commands::get_recorded_hotkey,
            commands::set_run_on_startup_cmd,
            commands::get_run_on_startup_cmd,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ─────────────────────────────────────────
//  Action helpers (called from tray + hotkey thread)
// ─────────────────────────────────────────
pub fn do_toggle_mute(app: &AppHandle) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap().clone();
    if let Ok(audio) = state.audio.try_lock() {
        if let Ok((muted, _debug)) = audio.toggle_mute(&cfg) {
            drop(audio);
            *state.is_muted.lock().unwrap() = muted;
            let peak = state.audio.lock().unwrap().get_peak_value().unwrap_or(0.0);
            update_tray_icon(app, muted);
            emit_state(app, muted, peak);
            trigger_osd(app, muted);
        }
    }
}

pub fn do_set_mute(app: &AppHandle, mute: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap().clone();
    let audio = state.audio.lock().unwrap();
    if audio.set_mute(mute, &cfg).is_ok() {
        drop(audio);
        *state.is_muted.lock().unwrap() = mute;
        let peak = state.audio.lock().unwrap().get_peak_value().unwrap_or(0.0);
        update_tray_icon(app, mute);
        emit_state(app, mute, peak);
        trigger_osd(app, mute);
    }
}

pub fn update_tray_icon(app: &AppHandle, is_muted: bool) {
    let is_light = utils::is_system_light_theme();
    let icon = load_tray_icon(is_muted, is_light);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_icon(Some(icon));
    }
}

pub fn trigger_osd(app: &AppHandle, is_muted: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock().unwrap();
    if !cfg.osd.enabled {
        return;
    }
    let duration = cfg.osd.duration;
    let size = cfg.osd.size;
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
                "Bottom" => mon_h - h - 100.0,
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
            serde_json::json!({ "is_muted": is_muted, "duration": duration }),
        );

        // Auto-hide after duration
        let win_clone = osd_win.clone();
        let dur = std::time::Duration::from_millis(duration as u64);
        std::thread::spawn(move || {
            std::thread::sleep(dur);
            let _ = win_clone.hide();
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
            let mut cfg = state.config.lock().unwrap();
            cfg.beep_enabled = !cfg.beep_enabled;
            cfg.save();
        }
        "toggle_osd" => {
            let mut cfg = state.config.lock().unwrap();
            cfg.osd.enabled = !cfg.osd.enabled;
            cfg.save();
        }
        "toggle_overlay" => {
            let enabled = {
                let mut cfg = state.config.lock().unwrap();
                cfg.persistent_overlay.enabled = !cfg.persistent_overlay.enabled;
                cfg.save();
                cfg.persistent_overlay.enabled
            };
            if let Some(win) = app.get_webview_window("overlay") {
                if enabled {
                    let _ = win.show();
                } else {
                    let _ = win.hide();
                }
            }
        }
        "toggle_boot" => {
            let current = startup::get_run_on_startup();
            startup::set_run_on_startup(!current);
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
            let mut cfg = state.config.lock().unwrap();
            cfg.device_id = new_device_id.clone();
            cfg.save();
            drop(cfg);
            if let Ok(new_audio) = audio::AudioController::new(new_device_id.as_ref()) {
                *state.audio.lock().unwrap() = new_audio;
            }
        }
        _ => {}
    }
}
