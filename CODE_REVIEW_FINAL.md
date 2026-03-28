# MicMute-RS-Tauri Full Code Review

> **Date:** 2026-03-28
> **Verdict:** BLOCK -- 9 CRITICAL, 15 HIGH, 16 MEDIUM, 9 LOW issues
> **Purpose:** Detailed fix instructions for each issue, ordered by priority. An agent should work through these top-to-bottom.
> **Note:** This is the combined, finalized review merging previous findings with the latest code review pass.

---

## Table of Contents

1. [CRITICAL Issues](#critical-issues)
2. [HIGH Issues](#high-issues)
3. [MEDIUM Issues](#medium-issues)
4. [LOW Issues](#low-issues)
5. [Verification Checklist](#verification-checklist)

---

## CRITICAL Issues

### C1. Unsound `unsafe impl Send+Sync` for `AppState` -- COM STA Thread Violation

**File:** `src/backend/lib.rs:53-54`
**Risk:** Undefined behavior, silent data corruption

**Problem:** `AppState` contains COM interfaces (`IAudioEndpointVolume`, `IAudioMeterInformation`, `IAudioClient`) created on the main STA thread. Lines 53-54 declare:
```rust
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
```

The safety comment (lines 43-52) claims "All COM interfaces are accessed only from the main thread (STA)." But this is **false**: the hotkey polling thread (spawned at line 347) calls `do_toggle_mute()` and `do_set_mute()`, which lock `state.audio` and call COM methods like `IAudioEndpointVolume::SetMute` from that background thread. That thread initializes COM with `COINIT_MULTITHREADED` (line 358), not the same STA apartment.

**Fix Instructions:**
Option A (Recommended -- dispatch COM calls to main thread):
1. In `do_toggle_mute()` and `do_set_mute()` (lines 514-572), instead of directly locking `state.audio` and calling COM methods, use `app.run_on_main_thread()` to dispatch the audio COM calls back to the main thread.
2. Update the safety comment at lines 43-52 to accurately describe the new invariant.

---

### C2. Unsound PROPVARIANT Raw Pointer Cast

**File:** `src/backend/audio.rs:347-358`
**Risk:** Reading arbitrary memory, potential crash

**Problem:** The code manually casts a `PROPVARIANT` to raw pointers and reads at hardcoded offsets. This layout is not guaranteed by `windows-rs` and could change between crate versions or alignment settings.

**Fix Instructions:**
Replace lines 347-363 in `audio.rs` with safe PROPVARIANT access using the `windows` crate's typed union fields or `PropVariantToStringAlloc`. Also remove the now-unused constant `VT_LPWSTR` at line 39.

---

### C3. Command Injection via PowerShell String Interpolation

**File:** `src/backend/startup.rs:114-128`
**Risk:** Arbitrary command execution

**Problem:** The `create_task_elevated()` function formats a path into a PowerShell `-Command` string using single-quote delimiters. If `xml_path` contains a single quote, it breaks out of the PowerShell argument string.

**Fix Instructions:**
Escape single quotes for PowerShell by doubling them (`xml_path.replace('\'', "''")`) before passing to `format!`. Apply the same escaping to `delete_task_elevated()`.

---

### C4. XML Injection via USERNAME Environment Variable

**File:** `src/backend/startup.rs:77-82`
**Risk:** Task Scheduler XML manipulation

**Problem:** The `USERNAME` env var and `exe_path` are interpolated directly into XML via string replacement. A crafted username could inject arbitrary Task Scheduler XML elements.

**Fix Instructions:**
Add an `xml_escape` function to replace special characters (`&`, `<`, `>`, `"`, `'`) and apply it to all interpolated values before replacing the XML template placeholders. Fix the fallback in `env::current_exe()` to return early on error instead of using a relative path.

---

### C5. Mutex `.unwrap()` Throughout -- Panic Cascade on Poison

**Files:** `src/backend/lib.rs` (20+ sites), `src/backend/commands.rs` (30+ sites)
**Risk:** Full application crash if any thread panics while holding a mutex

**Problem:** Every mutex access uses `.lock().unwrap()`. If any thread panics while holding a mutex, the mutex becomes poisoned and all subsequent `.lock().unwrap()` calls crash.

**Fix Instructions:**
Switch from `std::sync::Mutex` to `parking_lot::Mutex` which never poisons. Add `parking_lot = "0.12"` to `Cargo.toml`, replace imports, and remove all `.unwrap()` calls after `.lock()`.

---

### C6. Google Fonts Import Blocked by Own CSP

**File:** `src/frontend/styles.css:4`
**Risk:** Functional bug -- Inter font never loads

**Problem:** Line 4 imports Google Fonts, but `tauri.conf.json:58` blocks `fonts.googleapis.com`. The font never loads.

**Fix Instructions:**
Remove the Google Fonts import and update the `--font` CSS variable to use only system fonts (`-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`).

---

### C7. `'unsafe-inline'` in CSP `style-src`

**File:** `tauri.conf.json:58`
**Risk:** CSS injection attacks, weakened security posture

**Problem:** The CSP includes `style-src 'self' 'unsafe-inline'`. This allows any injected `<style>` tag or inline `style=` attribute to execute.

**Fix Instructions:**
Move any inline `<style>` content to external CSS files, then remove `'unsafe-inline'` from `tauri.conf.json`.

---

### C8. `withGlobalTauri: true` Exposes Full IPC Surface

**File:** `tauri.conf.json:56`
**Risk:** Any JS execution in WebView has full access to all Tauri commands

**Problem:** `"withGlobalTauri": true` injects the entire Tauri API into `window.__TAURI__` globally. Combined with `'unsafe-inline'` in CSP, a single XSS gives full backend access.

**Fix Instructions:**
Document as tech debt for now, but a proper fix involves setting `"withGlobalTauri": false`, installing `@tauri-apps/api` via npm, and adding a JS bundler.

---

### C9. XSS Vulnerability Risks via DOM Mutation (`innerHTML`)

**File:** `src/frontend/js/main.js:176, 192, 218`
**Risk:** Cross-Site Scripting (XSS) via insecure mutation pattern

**Problem:** `innerHTML` is used for clearing containers (`container.innerHTML = ""`) and rendering options (`sel.innerHTML = ...`). Although currently used with hardcoded strings, relying on `innerHTML` is an insecure pattern that easily introduces XSS. The `CHANGELOG.md` claims this was fixed, but the code still contains it.

**Fix Instructions:**
Replace `innerHTML` assignments with safer DOM APIs:
- For clearing containers: use `container.replaceChildren()` or `container.textContent = ""`
- For adding the default option in `rebuildDeviceSelect()`:
```js
sel.textContent = "";
const defaultOpt = document.createElement("option");
defaultOpt.value = "";
defaultOpt.textContent = "Default Windows Device";
sel.appendChild(defaultOpt);
```

---

## HIGH Issues

### H1. No Path Traversal Validation for Sound File Paths

**File:** `src/backend/audio.rs:244-283`
**Problem:** `sound_config.file` from `AppConfig` is used to construct file paths without validating for path traversal (`..`).
**Fix:** Add an `is_safe_sound_path` helper that rejects paths with `..` components and call it in `play_feedback()`.

### H2. COM `PWSTR` from `GetId()` Never Freed -- Memory Leak

**File:** `src/backend/audio.rs:166-167, 342-343`
**Problem:** `IMMDevice::GetId()` returns a COM-allocated `PWSTR` that must be freed with `CoTaskMemFree`. The code converts it to a String but never frees the original.
**Fix:** Wrap the pointer in `CoTaskMemGuard` immediately after receiving it (verify with the active `windows` crate version first if Drop is natively implemented).

### H3. Unbounded `mpsc::channel` for Audio Feedback

**File:** `src/backend/lib.rs:239`
**Problem:** The unbounded channel can cause memory growth under rapid input.
**Fix:** Replace `std_mpsc::channel` with `std_mpsc::sync_channel::<AudioFeedbackMsg>(1)` and change `send()` to `try_send()`.

### H4. Unbounded Thread Spawning per OSD Trigger

**File:** `src/backend/lib.rs:630-636`
**Problem:** Each OSD trigger spawns a new OS thread.
**Fix:** Implement a single persistent OSD timer worker thread fed by a channel, similar to the audio feedback worker.

### H5. Triple Config Mutex Lock with TOCTOU / Deadlock Risk

**File:** `src/backend/commands.rs:147-187`
**Problem:** `update_config` acquires the config lock three separate times, exposing intermediate states.
**Fix:** Refactor `update_config` to hold a single lock for the config read-modify-write operation.

### H6. `console.log` in Production Frontend Code

**Files:** `src/frontend/js/main.js`, `src/frontend/js/overlay.js`
**Problem:** Contains 12 instances of `console.log`. The worst instance constantly overwrites the UI footer with debug info during VU polling.
**Fix:** Remove all `console.log` statements. Keep `console.error` for genuine error handlers.

### H7. Pervasive Direct Mutation of Shared `config` Object

**Files:** `src/frontend/js/main.js`, `src/frontend/js/overlay.js`
**Problem:** The global `config` object is mutated in-place everywhere, creating race conditions with `config-update` events.
**Fix:** Implement an `isSaving` guard in `main.js` to block `config-update` overwrites while `saveConfig()` is in flight.

### H8. `main.js` is 826 Lines -- Exceeds 800-Line Limit

**File:** `src/frontend/js/main.js`
**Problem:** The main frontend script violates the structural code limits (>800 lines).
**Fix:** Extract into focused modules (e.g. `hotkey-ui.js`, `audio-ui.js`, `ui-helpers.js`).

### H9. `setupEventListeners()` is 170 Lines

**File:** `src/frontend/js/main.js:383-554`
**Problem:** Monolithic event binding function.
**Fix:** Break into smaller functions (`bindTabListeners`, `bindAudioListeners`, etc.).

### H10. VU Poll Timer Never Cleared Before Re-creation

**File:** `src/frontend/js/main.js:664-665`
**Problem:** Calling `startVuPoll()` repeatedly creates dangling interval timers.
**Fix:** Add `if (vuPollTimer) clearInterval(vuPollTimer);`.

### H11. Tauri `listen()` Unlisten Handles Discarded

**Files:** `src/frontend/js/main.js`, `src/frontend/js/overlay.js`
**Problem:** The unlisten functions returned by `await listen(...)` are never stored or invoked.
**Fix:** Store the handles in module-level variables and call them before re-listening.

### H12. `autoFitWindow()` DOM Mutation Not in try/finally

**File:** `src/frontend/js/main.js:883-900`
**Problem:** DOM styles are temporarily modified for measurement but restoration is not guaranteed on exception.
**Fix:** Wrap the measurement block in a `try...finally` to ensure styles are restored.

### H13. Dead `window.switchTab` Global Export

**File:** `src/frontend/js/main.js:929`
**Problem:** Unused global export.
**Fix:** Remove `window.switchTab = switchTab;`.

### H14. Missing `// SAFETY:` Comment on Unsafe Dereference

**File:** `src/backend/hotkey.rs:195`
**Problem:** `unsafe` block without justification comment.
**Fix:** Add `// SAFETY:` explaining the Windows message context guarantee.

### H15. COM `PWSTR::to_string()` Errors Silently Discarded

**File:** `src/backend/audio.rs:167, 343`
**Problem:** `unwrap_or_default()` yields an empty string on error, which can match a blank device ID.
**Fix:** Use `match` to properly log the error and `continue` on failure.

---

## MEDIUM Issues

### M1. `run()` Function is ~336 Lines
Extract into smaller functions: `init_audio`, `init_hotkeys`, `spawn_hotkey_loop`.

### M2. No Size Limit on `update_config` JSON Payload
Add a size limit (e.g. `64 * 1024` bytes) check before deserialization.

### M3. `osd.js` Doesn't Cancel Previous Hide Timer
Clear the previous timeout before showing the OSD again.

### M4. `mousemove` Drag Detection Unthrottled
Add simple debouncing/throttling (e.g., 50ms) to the mousemove listener in `overlay.js`.

### M5. Missing `<option value>` Attributes in HTML
Add explicit `value` attributes to all `<option>` elements to guarantee deterministic JS lookups.

### M6. `config.save()` Swallows I/O Errors
Change `save()` to return `Result<(), String>` and handle/log the error in the callers.

### M7. `set_mute` Command Returns Generic Error String
Propagate the actual COM error message instead of ignoring `is_ok()`.

### M8. Global Statics in `hotkey.rs` Prevent Safe Re-initialization
Replace the silent `let _ =` assignment with an `expect()` to panic correctly on invalid double-initialization.

### M9. `preview_audio_feedback` Accepts Full Config Blob Without Size Limit
Add a max size check (e.g. `64 * 1024` bytes) before deserializing.

### M10. `update_config` Re-Locks Config
*(Resolved concurrently with H5)*.

### M11. `current_exe()` Failure Silently Falls Back to Relative Path
Update the fallback logic to abort gracefully instead of using an invalid relative fallback.

### M12. `startRecording` Safety Timeout Not Cancellable
Store the timeout handle and clear it in `finishRecording` and `cancelRecording`.

### M13. Missing `<meta name="viewport">` in OSD and Overlay HTML
Add standard viewport meta tags to the frontend HTML headers.

### M14. No Accessibility Labels on Interactive Elements
Add `aria-label` and `aria-pressed` to interactive buttons like `#btn-toggle-mute` in `index.html` and update them dynamically in `main.js`.

### M15. Hardcoded Emojis in Code/Comments
**File:** `src/frontend/js/main.js:634` (and `index.html`)
**Problem:** Text emojis like `"🔇"` and `"🎤"` are hardcoded, causing inconsistent rendering across OS environments.
**Fix:** Use standard icon libraries or the existing inline SVGs (in `src/frontend/assets/`) instead of hardcoded text emojis.

### M16. Missing Automated Tests
**File:** Project-wide
**Problem:** Relying entirely on manual bin tests.
**Fix:** Implement unit testing on pure Rust logic (e.g. `config.rs`, string manipulation, URL validators) and setup automated Playwright E2E tests.

---

## LOW Issues

### L1. `cargo fmt` Fails in 9 Files
Run `cargo fmt`.

### L2. 29 Clippy Warnings
Run `cargo clippy --fix --allow-dirty -- -D warnings`.

### L3. `VK_NAMES` Object Rebuilt Per Call
Move the `VK_NAMES` object definition to module-level scope instead of recreating it inside the `vkToName` function.

### L4. CSS `transition: all` Affects Unintended Properties
Be specific with CSS transitions (`transition: background-color .18s ease, ...`) instead of `all`.

### L5. `#[allow(unused_imports)]` in `com_interfaces.rs`
Remove `unused_imports` and manually clean up the warnings.

### L6. Full `AppConfig` Cloned on Every Mute Toggle
Refactor to pass only needed parameters instead of cloning the entire large configuration struct on each toggle.

### L7. `showDebug()` Timer Conflicts
Clear existing timeouts before assigning a new one in `showDebug()`.

### L8. `setSelect()` Iterates All Options Instead of Using `.value`
Change the function to directly set `el.value = value;` instead of looping through options.

### L9. Capabilities `platforms` Includes Linux/macOS
Remove Linux and macOS from the `capabilities/default.json` as this is a Windows-only tool.

---

## Verification Checklist

After applying all fixes, verify:

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo build --release` succeeds
- [ ] Application launches without memory leaks and COM works
- [ ] XSS vulnerabilities on settings tab mitigated (No `innerHTML`)
- [ ] Settings window opens cleanly and persists options across reboots
- [ ] Hotkey toggles work smoothly without unbounded thread or memory channel leaks
- [ ] Device switching functions correctly
- [ ] "Start on Boot" manages elevated Task Scheduler safely (No command injection)
- [ ] Fonts and CSS load properly following CSP guidelines
- [ ] Zero unhandled `console.log` data dumps to the user UI
- [ ] Emojis replaced with proper SVG icons
- [ ] Code is modular (Files > 800 lines split, Functions > 50 lines refactored)
