# MicMuteRs

A lightweight Windows microphone mute utility built with Rust and [Tauri v2](https://v2.tauri.app/).

> This is a rewrite of [MicMute (Python)](https://github.com/madbeat14/MicMute), rebuilt from the ground up in Rust for lower resource usage and deeper Windows integration.

## Features

- **Global hotkey** — Toggle or separate mute/unmute with any key (media keys supported)
- **System tray** — Mute/unmute from the tray icon; left-click toggles, right-click opens menu
- **Always-on-top overlay** — Persistent draggable indicator showing mute state and voice activity
- **On-screen display (OSD)** — Transient notification popup on mute toggle
- **Multi-monitor support** — Independent overlay and OSD on up to two monitors with per-monitor settings
- **Multi-device sync** — Control multiple microphones simultaneously
- **AFK auto-mute** — Automatically mutes after configurable idle timeout, restores on activity
- **Audio feedback** — Beep or custom WAV sound on mute/unmute with adjustable volume
- **Run on startup** — Windows Task Scheduler integration (synced between tray menu and settings)
- **Theme detection** — Auto-detects system light/dark theme; overlay analyzes screen pixels behind it for optimal icon color
- **Portable** — Single executable, no installer required

## Installation

### Download

Download the latest installer or standalone executable from the [Releases](https://github.com/madbeat14/MicMute-RS-Tauri/releases) page:

| File | Description |
|------|-------------|
| `MicMuteRs_x.x.x_x64-setup.exe` | NSIS installer (recommended) |
| `MicMuteRs_x.x.x_x64_en-US.msi` | MSI installer |
| `mic-mute-rs.exe` | Standalone portable executable |

### Build from source

Requires [Rust](https://rustup.rs/) (stable).

```bash
git clone https://github.com/madbeat14/MicMute-RS-Tauri.git
cd MicMute-RS-Tauri
cargo build --release
```

The executable will be at `target/release/mic-mute-rs.exe`.

## Usage

1. Run `mic-mute-rs.exe` — it starts minimized to the system tray
2. Click the tray icon to open Settings
3. Configure your preferences across the available tabs:

| Tab | Options |
|-----|---------|
| **Devices** | Select primary microphone, add sync devices |
| **Audio** | Enable beep or custom WAV feedback, adjust volume/frequency |
| **Hotkeys** | Toggle or separate mute/unmute hotkeys |
| **Overlay** | Per-monitor overlay — size, opacity, lock position, theme |
| **OSD** | Per-monitor OSD — duration, size, position |
| **System & Startup** | Run on login, AFK auto-mute timeout |

## Configuration

Settings are saved to `%LOCALAPPDATA%\MicMute\mic_config.json` and persist across restarts. Changes apply immediately.

Config is automatically migrated from older versions (flat overlay/OSD objects to per-monitor HashMaps).

## Tech Stack

- **Backend**: Rust + [Tauri v2](https://v2.tauri.app/)
- **Frontend**: Vanilla HTML/CSS/JS (no framework)
- **Audio**: Windows Core Audio COM APIs (`IAudioEndpointVolume`, `IAudioMeterInformation`)
- **Hotkeys**: Low-level keyboard hook (`WH_KEYBOARD_LL`) on dedicated thread
- **Audio playback**: [rodio](https://crates.io/crates/rodio)

## License

MIT
