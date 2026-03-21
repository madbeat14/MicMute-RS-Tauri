# Changelog

## [Unreleased]

### Restructure — `src/backend/` + `src/frontend/` layout

#### Folder Reorganization
- **Move Rust backend** from `src/` to `src/backend/` — all `.rs` files, `bin/` directory
- **Move vanilla JS frontend** from `ui/` to `src/frontend/` — HTML, JS, CSS, assets
- **JS files into `src/frontend/js/`** — `main.js`, `overlay.js`, `osd.js` moved to subdirectory
- **Delete leftover `frontend/`** — React/TypeScript migration remnants (node_modules, dist, etc.)
- **Clean up Python files** — removed `__pycache__` and `.py` test files from `bin/`

#### Module Extraction
- **New `theme.rs`** — extracted `is_system_light_theme()` and `is_background_light()` from `utils.rs`
- **Slimmed `utils.rs`** — now contains only `get_idle_duration`, `vk_to_string`, `set_click_through`, `force_topmost`

#### Bug Fixes
- **Fix COLORREF byte-order bug** in `is_background_light()` — was reading `0x00RRGGBB` instead of correct `0x00BBGGRR`, causing wrong overlay theme on colored backgrounds
- **Add COM memory RAII guard** (`CoTaskMemGuard`) in `audio.rs` — prevents leak if early return occurs after `GetMixFormat()`
- **Add URL scheme validation** in `open_url` command — only allows `http://` and `https://`
- **Add config validation** (`validate()` method) — clamps AFK timeout, overlay scale, OSD values to safe ranges
- **Log config save errors** instead of silently swallowing them

#### Code Quality
- **Replace magic number** `31` with named constant `VT_LPWSTR` in device enumeration
- **Flatten nested sync logic** — `sync_mute_to_devices()` extracted with early-return pattern
- **Replace innerHTML with DOM APIs** in `main.js` — prevents potential XSS in `rebuildSyncList` and `rebuildHotkeyRows`
- **Reduce VU poll intervals** — settings 50ms→100ms, overlay 60ms→100ms

### Performance - RAM & CPU Optimization

#### Memory Leak Fixes
- **Fix audio sink leak**: Replaced `sink.detach()` with `sink.sleep_until_end()` in audio feedback playback. Detached sinks were never explicitly dropped, causing memory accumulation on rapid mute toggling.
- **Single audio worker thread**: Replaced per-toggle `std::thread::spawn` (~1MB stack each) with a persistent worker thread (256KB stack) fed via `mpsc` channel. Covers all mute toggle, set mute, and audio preview paths.
- **OSD generation tracking**: Added atomic generation counter to prevent stale OSD hide threads from incorrectly hiding newer OSD popups.

#### CPU Reduction
- **Cache hotkey config**: Stopped cloning the full `hotkey` HashMap and `hotkey_mode` String every 10ms poll iteration. Now caches plain `u32`/`bool` values, refreshed every ~500ms.
- **Throttle AFK check**: Moved AFK idle detection from every 10ms to every ~1 second (AFK timeouts are measured in seconds).
- **Reduce config mutex contention**: From 3 mutex locks per 10ms iteration down to 1 lock per 10ms (hotkey receiver) + 1 lock per 500ms (config refresh).
- **Lower process priority**: Changed from `HIGH_PRIORITY_CLASS` to `ABOVE_NORMAL_PRIORITY_CLASS` — less aggressive CPU scheduling for a utility app.

#### RAM Reduction
- **Remove unused `image` crate**: Only `tauri::image::Image` was used, not the standalone `image` crate.
- **Remove unused `lazy_static` crate**: Not referenced anywhere in source.
- **Reduce audio client buffer**: From 1 second to 100ms — only needs to feed the peak meter, not record audio.
- **Eliminate debug string allocations**: `set_mute()` and `toggle_mute()` no longer build `String` with `format!()` for debug messages that were discarded. Return types simplified.
- **Remove production `eprintln!` output**: `get_cached_devices` was writing debug info to stderr on every call.

#### Audio Feedback Fix
- **Instant sound on rapid toggles**: Audio feedback no longer queues up when clicking rapidly. `play_feedback` returns the `Sink` to the worker thread, which holds it alive until the next toggle arrives — dropping the old Sink instantly stops the previous sound and starts the new one with no delay.

#### Hotkey Reliability Fix
- **Dual-layer hotkey interception**: Added `RegisterHotKey` as a backup alongside the existing `WH_KEYBOARD_LL` hook. When the tray context menu opens a modal loop, Windows can silently remove the LL hook, causing hotkeys to pass through (e.g., Play/Pause triggers the media player instead of muting). `RegisterHotKey` is processed by the window manager independently of the hook chain, so it catches keys even during modal loops. Deduplication via tick-count comparison prevents double-firing when both mechanisms are active. Hook reinstall every ~500ms is retained as an additional safety net.
