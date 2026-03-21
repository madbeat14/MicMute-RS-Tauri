# Changelog

## [Unreleased]

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
- **Reinstall keyboard hook periodically**: Windows can silently remove `WH_KEYBOARD_LL` hooks when the hook thread doesn't respond within the system timeout (e.g., during a tray context menu modal loop). The hook is now reinstalled every ~500ms via a custom `WM_REINSTALL_HOOK` message posted to the hook thread, ensuring hotkeys keep working even while the tray menu is open.
