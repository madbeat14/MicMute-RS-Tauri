# Feature Plan: Multi-Monitor Overlay & OSD Support

## Goal

Allow the user to show overlay and/or OSD windows on **multiple monitors independently**, each with its own configuration (enabled, position, size, opacity, theme, etc.). The settings UI should provide a **monitor selector dropdown** in the Overlay and OSD tabs; selecting a different monitor displays and edits that monitor's settings.

---

## Current Architecture (Single Monitor)

### Config (`config.rs`)
- `AppConfig.persistent_overlay` → single `OverlayConfig` struct
- `AppConfig.osd` → single `OsdConfig` struct

### Tauri Windows (`tauri.conf.json`)
- One `overlay` window (label: `"overlay"`)
- One `osd` window (label: `"osd"`)

### Backend (`lib.rs`, `commands.rs`)
- `trigger_osd()` positions the single `osd` window on `current_monitor()`
- `update_config()` manages a single `overlay` window (show/hide, position, scale)
- `spawn_hotkey_loop()` re-asserts topmost on the single `overlay` window
- `save_overlay_position()` saves x/y for a single overlay

### Frontend
- `main.js` → `bindOverlayListeners()` and `bindOsdListeners()` bind directly to single config fields
- `overlay.js` → reads `config.persistent_overlay` (singular)
- `osd.js` → reads from `osd-show` event payload

---

## Proposed Design

### 1. Config Schema Changes (`config.rs`)

#### New: Per-Monitor Overlay Config
```rust
/// Identifies a monitor by name (from Tauri's Monitor::name())  
/// The "primary" sentinel is used for the primary/main monitor.
pub type MonitorId = String;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct OverlayConfig {
    // All existing fields remain identical:
    pub enabled: bool,
    pub show_vu: bool,
    pub opacity: u8,
    pub x: i32,
    pub y: i32,
    pub position_mode: String,
    pub locked: bool,
    pub sensitivity: u32,
    pub device_id: Option<String>,
    pub scale: u32,
    pub theme: String,
}
```

#### New: Per-Monitor OSD Config
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct OsdConfig {
    // All existing fields remain identical:
    pub enabled: bool,
    pub duration: u32,
    pub position: String,
    pub size: u32,
    pub opacity: u8,
}
```

#### AppConfig Changes
```rust
pub struct AppConfig {
    // ... existing fields ...

    // REMOVE these two flat fields:
    // pub persistent_overlay: OverlayConfig,
    // pub osd: OsdConfig,

    // REPLACE with per-monitor maps:
    /// Key = monitor name (e.g. "\\.\DISPLAY1", "DELL U2723QE").
    /// A special key "primary" is the default/fallback.
    pub persistent_overlay: HashMap<String, OverlayConfig>,
    pub osd: HashMap<String, OsdConfig>,
}
```

#### Migration Strategy (Backward Compatibility)
- Serde's `#[serde(default)]` + a custom deserializer or post-load migration:
  - If the JSON has `persistent_overlay` as an **object with `enabled` field** (old format), wrap it into `HashMap { "primary" => old_value }`.
  - Same for `osd`.
- The `AppConfig::load()` method already has a migration section; add a step there.
- On next `save()`, the new format is written.

```rust
impl AppConfig {
    pub fn load() -> Self {
        // ... existing load logic ...
        
        // Migration: if persistent_overlay was loaded as empty map but
        // the raw JSON contains old-style flat object, migrate it
        if config.persistent_overlay.is_empty() {
            // try parsing the raw JSON value for "persistent_overlay" as OverlayConfig
            if let Some(raw) = raw_json.get("persistent_overlay") {
                if raw.get("enabled").is_some() {
                    // Old flat format
                    let old: OverlayConfig = serde_json::from_value(raw.clone()).unwrap_or_default();
                    config.persistent_overlay.insert("primary".to_string(), old);
                    needs_save = true;
                }
            }
        }
        // Same for osd...
    }
}
```

#### Default Implementation
```rust
impl Default for AppConfig {
    fn default() -> Self {
        let mut overlay_map = HashMap::new();
        overlay_map.insert("primary".to_string(), OverlayConfig::default());
        let mut osd_map = HashMap::new();
        osd_map.insert("primary".to_string(), OsdConfig::default());
        
        Self {
            // ...
            persistent_overlay: overlay_map,
            osd: osd_map,
        }
    }
}
```

---

### 2. New Tauri Command: `get_monitors` (`commands.rs`)

Expose available monitors to the frontend for the dropdown:

```rust
#[derive(Serialize, Clone)]
pub struct MonitorDto {
    pub name: String,       // e.g. "\\.\DISPLAY1" or friendly name
    pub width: u32,
    pub height: u32,
    pub x: i32,             // physical position
    pub y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

#[tauri::command]
pub async fn get_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorDto>, String> {
    use tauri::Manager;
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let primary = app.primary_monitor().map_err(|e| e.to_string())?;
    let primary_name = primary.as_ref().and_then(|m| m.name().map(|n| n.to_string()));
    
    Ok(monitors.iter().map(|m| {
        let name = m.name().map(|n| n.to_string()).unwrap_or_else(|| "Unknown".to_string());
        let size = m.size();
        let pos = m.position();
        MonitorDto {
            is_primary: primary_name.as_deref() == Some(&name),
            name,
            width: size.width,
            height: size.height,
            x: pos.x,
            y: pos.y,
            scale_factor: m.scale_factor(),
        }
    }).collect())
}
```

Register in `generate_handler![]` in `lib.rs`.

Add to `capabilities/default.json`:
```json
"core:window:allow-available-monitors",
"core:window:allow-primary-monitor"
```

---

### 3. Dynamic Overlay Windows (`lib.rs`)

Currently, there is a single `overlay` window defined in `tauri.conf.json`. For multi-monitor support, we need **one overlay window per enabled monitor**.

#### Approach: Dynamic Window Creation

- **Keep the static `overlay` window in `tauri.conf.json`** as a template/fallback for backwards compatibility, but hide it at startup.
- At startup and whenever config changes, dynamically create/destroy overlay windows using unique labels like `overlay-<monitor_name>` (sanitized).
- Use `tauri::WebviewWindowBuilder` to create windows at runtime with the same properties as the static one.

```rust
// Helper to create an overlay window for a specific monitor
fn create_overlay_window(app: &AppHandle, monitor_key: &str, cfg: &OverlayConfig, monitor_pos: (i32, i32)) -> Result<(), String> {
    let label = format!("overlay-{}", sanitize_label(monitor_key));
    
    // Skip if window already exists
    if app.get_webview_window(&label).is_some() {
        return Ok(());
    }
    
    let win = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App("overlay.html".into()),
    )
    .title("MicMuteRs Overlay")
    .inner_size(cfg.scale as f64, cfg.scale as f64)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .visible(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|e| e.to_string())?;
    
    // Position on the target monitor
    let _ = win.set_position(tauri::PhysicalPosition::new(
        monitor_pos.0 + cfg.x,
        monitor_pos.1 + cfg.y,
    ));
    
    let _ = win.show();
    Ok(())
}

/// Sanitize a monitor name to be a valid Tauri window label
fn sanitize_label(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
```

#### Sync Function: `sync_overlay_windows()`

Called from `update_config()` and at startup:

```rust
fn sync_overlay_windows(app: &AppHandle, config: &AppConfig) {
    let monitors = app.available_monitors().unwrap_or_default();
    
    // Build set of labels that SHOULD exist
    let mut desired_labels: HashSet<String> = HashSet::new();
    
    for (monitor_key, overlay_cfg) in &config.persistent_overlay {
        if !overlay_cfg.enabled {
            continue;
        }
        let label = format!("overlay-{}", sanitize_label(monitor_key));
        desired_labels.insert(label.clone());
        
        // Find the monitor's physical position
        let mon_pos = find_monitor_position(&monitors, monitor_key);
        
        if let Some(win) = app.get_webview_window(&label) {
            // Update existing window
            apply_overlay_config(&win, overlay_cfg, mon_pos);
        } else {
            // Create new window
            let _ = create_overlay_window(app, monitor_key, overlay_cfg, mon_pos);
        }
    }
    
    // Destroy overlay windows that are no longer needed
    // (iterate all windows matching "overlay-*" pattern)
    for win in app.webview_windows().values() {
        let label = win.label().to_string();
        if label.starts_with("overlay-") && !desired_labels.contains(&label) {
            let _ = win.destroy();
        }
    }
    
    // Hide the legacy static "overlay" window  
    if let Some(legacy) = app.get_webview_window("overlay") {
        let _ = legacy.hide();
    }
}
```

---

### 4. Dynamic OSD Windows (`lib.rs`)

Same pattern as overlay: create OSD windows per enabled monitor.

#### `trigger_osd()` Changes

```rust
pub fn trigger_osd(app: &AppHandle, is_muted: bool) {
    let state: tauri::State<Arc<AppState>> = app.state();
    let cfg = state.config.lock();
    let monitors = app.available_monitors().unwrap_or_default();
    
    for (monitor_key, osd_cfg) in &cfg.osd {
        if !osd_cfg.enabled {
            continue;
        }
        
        let label = format!("osd-{}", sanitize_label(monitor_key));
        let duration = osd_cfg.duration;
        let size = osd_cfg.size;
        let opacity = osd_cfg.opacity;
        let position = osd_cfg.position.clone();
        
        // Find or create the OSD window for this monitor
        let osd_win = app.get_webview_window(&label).or_else(|| {
            create_osd_window(app, monitor_key, osd_cfg).ok();
            app.get_webview_window(&label)
        });
        
        if let Some(osd_win) = osd_win {
            // Position on the correct monitor
            let mon_pos = find_monitor_position(&monitors, monitor_key);
            position_osd_on_monitor(&osd_win, &position, size, mon_pos, &monitors, monitor_key);
            
            let _ = osd_win.show();
            let _ = osd_win.set_always_on_top(true);
            let _ = osd_win.emit(
                "osd-show",
                serde_json::json!({ "is_muted": is_muted, "duration": duration, "opacity": opacity }),
            );
            
            // Schedule hide
            schedule_osd_hide(osd_win, duration);
        }
    }
}
```

---

### 5. Overlay Position Saving Changes (`commands.rs`)

`save_overlay_position` needs to know **which** overlay window is saving:

```rust
#[tauri::command]
pub async fn save_overlay_position(
    state: State<'_, Arc<AppState>>,
    monitor_key: String,  // NEW parameter
    x: i32,
    y: i32,
) -> Result<(), String> {
    let mut cfg = state.config.lock();
    if let Some(overlay_cfg) = cfg.persistent_overlay.get_mut(&monitor_key) {
        overlay_cfg.x = x;
        overlay_cfg.y = y;
    }
    if let Err(e) = cfg.save() {
        tracing::error!("{}", e);
    }
    Ok(())
}
```

#### `overlay.js` Changes

The overlay window needs to know its `monitor_key` so it can pass it to `save_overlay_position`. This can be done via:
1. **URL query parameter**: When creating the window, set the URL to `overlay.html?monitor=<key>`.
2. The overlay JS reads `new URLSearchParams(window.location.search).get('monitor')` on init.

```javascript
// overlay.js
const monitorKey = new URLSearchParams(window.location.search).get('monitor') || 'primary';

async function saveCurrentPosition() {
    const { getCurrentWindow } = window.__TAURI__.window;
    const win = getCurrentWindow();
    const position = await win.outerPosition();
    await invoke("save_overlay_position", { monitorKey, x: position.x, y: position.y });
}
```

---

### 6. `update_config()` Changes (`commands.rs`)

The current `update_config()` manages a single overlay window. It needs to be updated to call `sync_overlay_windows()` and `sync_osd_windows()` instead:

```rust
// In update_config(), REPLACE the overlay block (lines 203-256) with:

// UPDATE OVERLAY & OSD WINDOWS (multi-monitor)
sync_overlay_windows(&app, &new_config);
sync_osd_windows(&app, &new_config);
```

The `save_overlay_position` preserve logic needs updating:
```rust
// Preserve overlay positions from dragging (per-monitor)
for (key, overlay_cfg) in &cfg.persistent_overlay {
    if let Some(new_overlay) = new_config.persistent_overlay.get_mut(key) {
        new_overlay.x = overlay_cfg.x;
        new_overlay.y = overlay_cfg.y;
    }
}
```

---

### 7. `spawn_hotkey_loop()` Changes (`lib.rs`)

The topmost re-assertion loop needs to iterate **all** overlay windows:

```rust
// REPLACE the single overlay topmost block with:
if cached_overlay_enabled {
    for (label, win) in app_handle.webview_windows() {
        if label.starts_with("overlay-") {
            if let Ok(tauri_hwnd) = win.hwnd() {
                let hwnd = HWND(tauri_hwnd.0);
                crate::utils::force_topmost(hwnd);
            }
        }
    }
}
```

`cached_overlay_enabled` should become `true` if **any** monitor has overlay enabled:
```rust
cached_overlay_enabled = st.persistent_overlay.values().any(|o| o.enabled);
```

---

### 8. `get_overlay_background_is_light()` Changes (`commands.rs`)

This needs to work per overlay window. Option: accept a `monitor_key` parameter, or have it look up the caller window:

```rust
#[tauri::command]
pub async fn get_overlay_background_is_light(
    app: tauri::AppHandle,
    monitor_key: Option<String>,
) -> Result<bool, String> {
    let label = match monitor_key {
        Some(key) => format!("overlay-{}", crate::sanitize_label(&key)),
        None => "overlay".to_string(),  // legacy fallback
    };
    
    if let Some(overlay_win) = app.get_webview_window(&label) {
        let tauri_hwnd = overlay_win.hwnd().map_err(|e| e.to_string())?;
        let hwnd = HWND(tauri_hwnd.0);
        Ok(crate::theme::is_background_light(hwnd))
    } else {
        Ok(crate::theme::is_system_light_theme())
    }
}
```

---

### 9. Frontend: Settings UI Changes

#### 9.1 Overlay Tab (`index.html`)

Add a **monitor selector dropdown** at the top of the Overlay card:

```html
<div id="tab-overlay" class="tab-pane">
  <section class="card" id="sec-overlay">
    <div class="card-header">
      <span>🪟 Persistent Overlay</span>
    </div>
    <div class="card-body">
      <!-- NEW: Monitor selector -->
      <label class="field-label">Monitor</label>
      <select id="sel-overlay-monitor" class="select-input"></select>
      
      <!-- Existing controls remain, but now apply to selected monitor -->
      <label class="toggle-row mt-8">
        <span>Enable Overlay</span>
        <input type="checkbox" id="chk-overlay" class="toggle-switch" />
      </label>
      <!-- ... rest of overlay options ... -->
    </div>
  </section>
</div>
```

#### 9.2 OSD Tab (`index.html`)

Same pattern:
```html
<div id="tab-osd" class="tab-pane">
  <section class="card" id="sec-osd">
    <div class="card-header">
      <span>📺 On-Screen Display (OSD)</span>
    </div>
    <div class="card-body">
      <!-- NEW: Monitor selector -->
      <label class="field-label">Monitor</label>
      <select id="sel-osd-monitor" class="select-input"></select>
      
      <label class="toggle-row mt-8">
        <span>Enable OSD</span>
        <input type="checkbox" id="chk-osd" class="toggle-switch" />
      </label>
      <!-- ... rest of OSD options ... -->
    </div>
  </section>
</div>
```

#### 9.3 Frontend Logic (`main.js`)

##### New State Variables
```javascript
window.monitors = [];           // Array of MonitorDto from get_monitors
window.selectedOverlayMonitor = null;  // Currently selected monitor key for overlay tab
window.selectedOsdMonitor = null;      // Currently selected monitor key for OSD tab
```

##### Monitor Dropdown Population
```javascript
async function loadMonitors() {
    try {
        window.monitors = await invoke("get_monitors");
    } catch (e) {
        window.monitors = [];
    }
    rebuildMonitorSelects();
}

function rebuildMonitorSelects() {
    const overlaySelect = document.getElementById("sel-overlay-monitor");
    const osdSelect = document.getElementById("sel-osd-monitor");
    
    [overlaySelect, osdSelect].forEach(sel => {
        if (!sel) return;
        sel.textContent = "";
        for (const m of window.monitors) {
            const opt = document.createElement("option");
            // Use monitor name as the key
            opt.value = m.name;
            opt.textContent = m.is_primary 
                ? `${m.name} (Primary) — ${m.width}×${m.height}`
                : `${m.name} — ${m.width}×${m.height}`;
            sel.appendChild(opt);
        }
    });
    
    // Default to primary monitor or first available
    const primaryName = window.monitors.find(m => m.is_primary)?.name 
                     || window.monitors[0]?.name 
                     || "primary";
    
    if (!window.selectedOverlayMonitor) window.selectedOverlayMonitor = primaryName;
    if (!window.selectedOsdMonitor) window.selectedOsdMonitor = primaryName;
    
    if (overlaySelect) overlaySelect.value = window.selectedOverlayMonitor;
    if (osdSelect) osdSelect.value = window.selectedOsdMonitor;
}
```

##### Config ↔ UI Mapping (Overlay)

When the user selects a monitor in the overlay dropdown, the UI reads/writes from `config.persistent_overlay[monitorKey]`:

```javascript
function getOverlayConfigForSelectedMonitor() {
    const key = window.selectedOverlayMonitor || "primary";
    if (!window.config.persistent_overlay[key]) {
        // Create default config for this monitor
        window.config.persistent_overlay[key] = {
            enabled: false, show_vu: false, opacity: 80,
            x: 100, y: 100, position_mode: "Custom",
            locked: false, sensitivity: 5, device_id: null,
            scale: 100, theme: "Auto"
        };
    }
    return window.config.persistent_overlay[key];
}

function applyOverlayConfigToUI() {
    const ol = getOverlayConfigForSelectedMonitor();
    document.getElementById("chk-overlay").checked = ol.enabled;
    document.getElementById("chk-overlay-vu").checked = ol.show_vu;
    document.getElementById("chk-overlay-locked").checked = ol.locked;
    setSelect("sel-overlay-pos", ol.position_mode);
    setSelect("sel-overlay-theme", ol.theme);
    setSlider("slider-overlay-scale", ol.scale, "overlay-scale-val");
    setSlider("slider-overlay-opacity", ol.opacity, "overlay-opacity-val");
    setSlider("slider-overlay-sens", ol.sensitivity, "overlay-sens-val");
    updateSubOptions("chk-overlay", "overlay-options");
}
```

##### Updated Event Listeners (Overlay)

```javascript
function bindOverlayListeners() {
    // Monitor selector change
    document.getElementById("sel-overlay-monitor").addEventListener("change", e => {
        window.selectedOverlayMonitor = e.target.value;
        applyOverlayConfigToUI();
    });
    
    // All existing listeners now write to the selected monitor's config
    document.getElementById("chk-overlay").addEventListener("change", e => {
        getOverlayConfigForSelectedMonitor().enabled = e.target.checked;
        updateSubOptions("chk-overlay", "overlay-options");
        debouncedSave();
    });
    // ... same pattern for all other overlay controls ...
}
```

##### Same Pattern for OSD

```javascript
function getOsdConfigForSelectedMonitor() {
    const key = window.selectedOsdMonitor || "primary";
    if (!window.config.osd[key]) {
        window.config.osd[key] = {
            enabled: false, duration: 1500, position: "Bottom",
            size: 150, opacity: 80
        };
    }
    return window.config.osd[key];
}
```

##### Updated `applyConfigToUI()`

The main `applyConfigToUI()` function needs to call `applyOverlayConfigToUI()` and `applyOsdConfigToUI()` instead of directly reading flat fields.

---

### 10. Overlay JS per-window (`overlay.js`)

Each overlay window loads `overlay.html?monitor=<key>`. The JS reads:

```javascript
const monitorKey = new URLSearchParams(window.location.search).get('monitor') || 'primary';

// When reading config, use:
const myConfig = config.persistent_overlay[monitorKey] || config.persistent_overlay['primary'];
```

All references to `config.persistent_overlay` in `overlay.js` change to use `myConfig` instead.

---

### 11. OSD JS per-window (`osd.js`)

The OSD window receives everything it needs via the `osd-show` event payload, so **minimal changes** are needed. The event already sends `is_muted`, `duration`, and `opacity`. No monitor-specific config reading is required in `osd.js`.

---

### 12. Tray Menu Changes (`lib.rs`)

The tray menu currently has a single "Show Persistent Overlay" toggle. Options:
1. **Keep it simple**: Toggle overlays on ALL monitors at once.
2. **Submenu**: "Overlays" → per-monitor toggles.

**Recommended**: Option 1 for simplicity. The toggle enables/disables all configured overlays.

---

## File Change Summary

| File | Change Type | Description |
|------|-------------|-------------|
| `src/backend/config.rs` | **MODIFY** | Change `persistent_overlay` and `osd` from flat structs to `HashMap<String, _>`. Add migration logic. Update `validate()`. |
| `src/backend/commands.rs` | **MODIFY** | Add `get_monitors` command. Update `save_overlay_position` to accept `monitor_key`. Update `update_config` to call sync functions. Update `get_overlay_background_is_light` to accept `monitor_key`. |
| `src/backend/lib.rs` | **MODIFY** | Add `sync_overlay_windows()`, `sync_osd_windows()`, `create_overlay_window()`, `create_osd_window()`, `sanitize_label()`, `find_monitor_position()`. Update `trigger_osd()` to iterate all monitors. Update `spawn_hotkey_loop()` for multi-overlay topmost. Update startup overlay init. Register new command. |
| `src/backend/utils.rs` | No change | — |
| `src/backend/constants.rs` | No change | — |
| `src/frontend/index.html` | **MODIFY** | Add monitor selector dropdown to Overlay and OSD tabs. |
| `src/frontend/js/main.js` | **MODIFY** | Add monitor loading, per-monitor config read/write, update overlay/OSD listeners. |
| `src/frontend/js/overlay.js` | **MODIFY** | Read `monitor` query param, use per-monitor config, pass `monitorKey` to `save_overlay_position`. |
| `src/frontend/js/osd.js` | **MINIMAL** | Mostly unchanged; OSD event payload already contains all needed data. |
| `src/frontend/js/ui-helpers.js` | No change | — |
| `tauri.conf.json` | **MODIFY** | Optionally hide static `overlay`/`osd` windows at startup (set `visible: false` — already done). |
| `capabilities/default.json` | **MODIFY** | Add `core:window:allow-available-monitors`, `core:window:allow-primary-monitor`, and `core:webview:allow-create` permissions. |

---

## Implementation Order

1. **Config schema migration** (`config.rs`) — change types, add migration, update defaults and validation
2. **Backend `get_monitors` command** (`commands.rs`) — implement and register
3. **Dynamic window management** (`lib.rs`) — `sync_overlay_windows`, `sync_osd_windows`, update startup
4. **Update `trigger_osd()`** (`lib.rs`) — multi-monitor OSD triggering
5. **Update `save_overlay_position`** (`commands.rs`) — add `monitor_key` parameter
6. **Update `get_overlay_background_is_light`** (`commands.rs`) — add `monitor_key` parameter
7. **Update hotkey loop** (`lib.rs`) — iterate all overlay windows for topmost
8. **Update tray event handler** (`lib.rs`) — handle overlay toggle for all monitors
9. **Frontend: HTML** (`index.html`) — add monitor dropdowns
10. **Frontend: main.js** — monitor loading, per-monitor config binding
11. **Frontend: overlay.js** — per-window monitor key, read from HashMap
12. **Capabilities** (`capabilities/default.json`) — add required permissions
13. **Testing** — manual verification with 2+ monitors

---

## Edge Cases & Considerations

1. **Monitor names can change**: If a monitor is disconnected, its config entry persists but no window is created. On reconnection, it works again.
2. **"primary" key**: Always map the primary monitor. If the user has only one monitor, the experience is identical to today.
3. **Dynamic hotplug**: Monitors connected/disconnected at runtime won't auto-update. The user needs to open Settings and refresh. Could add a periodic monitor check later.
4. **Overlay position coordinates**: Currently stored as absolute physical pixels. Per-monitor configs should store **relative** positions (relative to the monitor's top-left corner). The `create_overlay_window` function adds the monitor's position offset.
5. **Window label sanitization**: Monitor names may contain special characters (`\\.\DISPLAY1`). Labels must be sanitized to alphanumeric + hyphens.
6. **Config size**: With many monitors, config grows linearly but remains small (< 10KB even with 10 monitors).
7. **OSD generation counter**: The `OSD_GENERATION` atomic needs to become per-window or per-label to avoid one monitor's hide canceling another's.
8. **Performance**: Creating/destroying Tauri webview windows is not free (~50-100ms each). Keep window creation to config changes only, not on every mute toggle. OSD windows should be created once and reused (show/hide pattern).

---

## Verification Plan

### Automated
- `cargo build` — ensure Rust compiles
- Existing config unit tests pass (update `test_config_default` and `test_config_validation` for new schema)

### Manual
1. Start app with old config file → verify migration creates `HashMap` with `"primary"` key
2. Open Settings → Overlay tab → see monitor dropdown with all connected monitors
3. Select Monitor 1 → enable overlay, set size 80px → verify overlay appears on Monitor 1
4. Select Monitor 2 → enable overlay, set size 120px → verify overlay appears on Monitor 2
5. Both overlays visible simultaneously with independent settings
6. Drag overlay on Monitor 2 → verify position persists after restart
7. Disable overlay for Monitor 1 → verify only Monitor 2's overlay remains
8. Test OSD: enable on both monitors → toggle mute → OSD appears on both
9. Disconnect a monitor → verify app doesn't crash, remaining overlays work
10. Reconnect monitor → verify overlay reappears after config change
