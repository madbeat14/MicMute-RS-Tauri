# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MicMute-RS-Tauri is a Windows microphone mute utility built with Tauri v2 (Rust backend + vanilla JS frontend). It features a system tray icon, persistent overlay window, on-screen display (OSD) notifications, global hotkeys, and deep Windows integration via COM/Core Audio APIs.

## Build & Run Commands

```bash
# Development with hot-reload (requires Tauri CLI)
cargo tauri dev

# Production build (creates NSIS installer)
cargo tauri build

# Run only the Rust backend
cargo run

# Diagnostic utilities
cargo run --bin diagnostics --release
cargo run --bin test_audio_playback --release
cargo run --bin test_direct_mute --release
cargo run --bin test_meter --release
cargo run --bin test_peak --release
```

There is no automated test suite — the `src/bin/` diagnostic binaries serve as manual integration tests.

## Architecture

### Windows & Processes

Three Tauri windows run simultaneously:
- **`settings`** (`ui/index.html`) — Main tabbed settings UI, hidden by default, shown on tray click
- **`overlay`** (`ui/overlay.html`) — Persistent 48×48 transparent always-on-top indicator, draggable
- **`osd`** (`ui/osd.html`) — Transient notification popup that auto-hides after a configured duration

### Backend Structure (`src/`)

| File | Purpose |
|------|---------|
| `lib.rs` | App entry, `AppState`, tray menu, hotkey polling loop, window event handling |
| `commands.rs` | All `#[tauri::command]` IPC handlers exposed to the frontend |
| `audio.rs` | Windows Core Audio (COM) integration — mute control, peak metering, audio playback |
| `config.rs` | `AppConfig` and sub-structs; serialized to `%APPDATA%\MicMute\mic_config.json` |
| `hotkey.rs` | Low-level keyboard hook (`WH_KEYBOARD_LL`) on dedicated high-priority thread |
| `startup.rs` | Windows Task Scheduler integration for run-on-login |
| `utils.rs` | Registry theme detection, screen pixel brightness analysis, idle time, VK→string |
| `com_interfaces.rs` | Custom `IPolicyConfig` COM interface for setting default audio devices |
| `constants.rs` | Hardcoded defaults (hotkey VK, frequencies, timeouts, overlay defaults) |

### State Management (Backend)

`AppState` (in `lib.rs`) is the single shared state, wrapped in `Arc<Mutex<T>>` and registered via `app.manage()`. Tauri injects it into commands via the `State<'_, AppState>` parameter. All config changes go through `update_config` command → `config.save()` → emit `config-update` event to all windows.

**Important**: `AppState` has manual `unsafe impl Send + Sync` because it holds Windows COM interfaces that must only be accessed from the main STA thread. COM calls in commands are safe only because Tauri commands run on the main thread for this app.

### IPC Patterns (Frontend ↔ Backend)

**Commands** (request/response):
```js
const { invoke } = window.__TAURI__.core;
const result = await invoke("command_name", { param: value });
```

**Events** (backend → all windows):
```js
const { listen } = window.__TAURI__.event;
await listen("state-update", ({ payload }) => { /* mute + peak */ });
await listen("config-update", ({ payload }) => { /* full AppConfig */ });
await listen("osd-show", ({ payload }) => { /* mute state */ });
```

Backend emits via `app.emit("event-name", payload)`.

### Frontend Structure (`ui/`)

Vanilla JS, no framework. Each HTML page is self-contained with its JS file:
- `main.js` — Settings page: `init()` loads state, `applyConfigToUI()` syncs config→UI, `saveConfig()` (debounced 300ms) syncs UI→backend
- `overlay.js` — Icon updates, drag handling, VU meter polling, 2-second config refresh
- `osd.js` — Listens for `osd-show`, displays card, fades out after duration

### Audio System

Uses Windows Core Audio COM interfaces directly (via the `windows` crate):
- `IMMDeviceEnumerator` → enumerate capture devices
- `IAudioEndpointVolume` → get/set mute state
- `IAudioMeterInformation` → read peak levels (0.0–1.0)
- `IAudioClient` → initialize hardware stream for metering

Audio playback (beep or custom WAV) uses `rodio` with a single `OutputStream` stored in `AppState`.

### Theme Detection

Two independent mechanisms:
1. **System theme**: Windows registry key `SystemUsesLightTheme` (used by settings UI)
2. **Overlay auto-theme**: Screen pixel capture behind the overlay window + brightness analysis (`utils::is_background_light`)

### Hotkey System

A dedicated thread installs `WH_KEYBOARD_LL`, runs a Windows message loop, and sends pressed VK codes via MPSC channel to the main hotkey polling loop (10ms interval in `lib.rs`). Recording mode is toggled via atomics; during recording, keypresses are consumed and not forwarded.

## Key Conventions

- Config is always loaded at startup, modified via `update_config` command, and saved to disk immediately. The full config is re-broadcast to all windows on every save.
- Tray icon and overlay icon both update on every mute state change; icon assets are embedded in the binary.
- The overlay saves its position via `save_overlay_position` command, called after drag ends (500ms debounce in `overlay.js`).
- AFK mute logic runs in the hotkey polling loop: if idle > configured timeout and not muted, auto-mutes; restores on activity.
- New Tauri commands must be registered in the `generate_handler![]` macro in `lib.rs`.
