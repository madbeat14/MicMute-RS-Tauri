// MicMuteRs – Settings Page Logic
// Uses window.__TAURI__ provided by Tauri to interact with the rust backend.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ──────────────────────────────────
//  State variables
// ──────────────────────────────────
let config = null;              // Holds the application settings object fetched from rust
let devices = [];               // Array of available audio devices
let isMuted = false;            // Current microphone active state
let recordingKey = null;        // Identifies which hotkey is currently being recorded (e.g. 'toggle' or 'mute')
let recordingPollTimer = null;  // Interval timer for checking if a new key was recorded
let vuPollTimer = null;         // Interval timer for the Volume Unit meter in the settings page

let saveTimeout = null;

/**
 * Debounces the saveConfig function, delaying the save operation 
 * to prevent excessive write calls to the backend when rapidly changing UI inputs.
 */
function debouncedSave() {
    if (saveTimeout) clearTimeout(saveTimeout);
    console.log("Scheduling config save...");
    saveTimeout = setTimeout(saveConfig, 300);
}

const COMMON_KEYS = [
    [0xB3, "Media Play/Pause"], [0x70, "F1"], [0x71, "F2"],
    [0x72, "F3"], [0x73, "F4"], [0x74, "F5"], [0x75, "F6"],
    [0x76, "F7"], [0x77, "F8"], [0x78, "F9"], [0x79, "F10"],
    [0x7A, "F11"], [0x7B, "F12"], [0x20, "Space"], [0x0D, "Enter"],
    [0xAD, "Volume Mute"], [0xAE, "Volume Down"], [0xAF, "Volume Up"],
];

// ──────────────────────────────────
//  Initialization
// ──────────────────────────────────

/**
 * Initializes the settings page. Loads the config, devices list, and mute state.
 * Sets up all UI bindings, polls, and subscriptions to real-time state updates.
 */
async function init() {
    try {
        config = await invoke("get_config");
    } catch (e) {
        console.error("init config error:", e);
    }

    try {
        isMuted = (await invoke("get_state")).is_muted;
    } catch (e) {
        console.error("init state error:", e);
    }

    try {
        // Use cached devices (populated during app startup) — avoids COM threading issues
        devices = (await invoke("get_cached_devices")).map(d => ({ id: d.id, name: d.name }));
    } catch (e) {
        console.error("init devices error:", e);
        // Fallback to ensuring at least an empty list
        devices = [];
    }

    applyConfigToUI();
    startVuPoll();
    setupEventListeners();
    setupHotkeyPassthrough();
    await listen("state-update", e => {
        isMuted = e.payload.is_muted;
        updateMuteUI(isMuted);
        updateVU(e.payload.peak_level);
    });

    // Listen for config updates from backend (e.g. from tray menu)
    await listen("config-update", e => {
        console.log("Received config-update", e.payload);
        config = e.payload.config;
        applyConfigToUI();
    });

    // Auto-fit window size to content on first load
    autoFitWindow();
}

// ──────────────────────────────────
//  Configuration to UI synchronization
// ──────────────────────────────────

/**
 * Reads the loaded `config` object and updates all UI elements
 * (checkboxes, sliders, selects, etc.) to reflect the current settings.
 */
function applyConfigToUI() {
    if (!config) return;

    // Devices
    rebuildDeviceSelect();
    rebuildSyncList();

    // Audio feedback
    document.getElementById("chk-beep").checked = config.beep_enabled;
    const mode = config.audio_mode || "beep";
    document.getElementById("radio-beep").checked = mode === "beep";
    document.getElementById("radio-custom").checked = mode === "custom";
    updateAudioModeUI(mode);

    // Custom sound paths
    document.getElementById("path-mute").value = config.sound_config?.mute?.file || "";
    document.getElementById("path-unmute").value = config.sound_config?.unmute?.file || "";

    // Volumes
    setSlider("slider-vol-mute", config.sound_config?.mute?.volume || 50, "vol-mute-val");
    setSlider("slider-vol-unmute", config.sound_config?.unmute?.volume || 50, "vol-unmute-val");

    // Beeps
    const bm = config.beep_config?.mute || { freq: 650, duration: 180, count: 2 };
    const bu = config.beep_config?.unmute || { freq: 700, duration: 200, count: 1 };
    document.getElementById("beep-mute-freq").value = bm.freq;
    document.getElementById("beep-mute-dur").value = bm.duration;
    document.getElementById("beep-mute-count").value = bm.count;
    document.getElementById("beep-unmute-freq").value = bu.freq;
    document.getElementById("beep-unmute-dur").value = bu.duration;
    document.getElementById("beep-unmute-count").value = bu.count;

    // Hotkeys
    document.getElementById("hk-mode-toggle").checked = config.hotkey_mode === "toggle";
    document.getElementById("hk-mode-sep").checked = config.hotkey_mode === "separate";
    rebuildHotkeyRows();

    // Overlay
    const ol = config.persistent_overlay;
    document.getElementById("chk-overlay").checked = ol.enabled;
    document.getElementById("chk-overlay-vu").checked = ol.show_vu;
    document.getElementById("chk-overlay-locked").checked = ol.locked;
    setSelect("sel-overlay-pos", ol.position_mode);
    setSelect("sel-overlay-theme", ol.theme);
    setSlider("slider-overlay-scale", ol.scale, "overlay-scale-val");
    setSlider("slider-overlay-opacity", ol.opacity, "overlay-opacity-val");
    setSlider("slider-overlay-sens", ol.sensitivity, "overlay-sens-val");
    updateSubOptions("chk-overlay", "overlay-options");

    // OSD
    const osd = config.osd;
    document.getElementById("chk-osd").checked = osd.enabled;
    setSlider("slider-osd-dur", osd.duration, "osd-dur-val");
    setSlider("slider-osd-size", osd.size, "osd-size-val");
    setSlider("slider-osd-opacity", osd.opacity, "osd-opacity-val");
    setSelect("sel-osd-pos", osd.position);
    updateSubOptions("chk-osd", "osd-options");

    // Startup / AFK
    invoke("get_run_on_startup_cmd").then(b => {
        document.getElementById("chk-startup").checked = b;
    });
    document.getElementById("chk-afk").checked = config.afk.enabled;
    setSlider("slider-afk-timeout", config.afk.timeout, "afk-timeout-val");
    updateSubOptions("chk-afk", "afk-timeout-row");

    // Mute status
    updateMuteUI(isMuted);
}

// ──────────────────────────────────
//  Device Selection Logic
// ──────────────────────────────────

/**
 * Rebuilds the primary audio device dropdown menu with options 
 * fetched from the rust backend. Selects the active configured device.
 */
function rebuildDeviceSelect() {
    const sel = document.getElementById("sel-device");
    sel.innerHTML = `<option value="">Default Windows Device</option>`;
    for (const d of devices) {
        const opt = document.createElement("option");
        opt.value = d.id;
        opt.textContent = d.name;
        if (config.device_id === d.id) opt.selected = true;
        sel.appendChild(opt);
    }
}

/**
 * Rebuilds the secondary devices list used for synchronizing mute 
 * status across multiple microphone inputs.
 */
function rebuildSyncList() {
    const container = document.getElementById("sync-list");
    container.innerHTML = "";
    const primaryId = config.device_id;
    for (const d of devices) {
        if (d.id === primaryId) continue;
        const isSynced = (config.sync_ids || []).includes(d.id);
        const label = document.createElement("label");
        const checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.dataset.syncId = d.id;
        checkbox.checked = isSynced;
        label.appendChild(checkbox);
        label.appendChild(document.createTextNode(" " + d.name));
        container.appendChild(label);
    }
}

// ──────────────────────────────────
//  Hotkey Configuration Logic
// ──────────────────────────────────

/**
 * Dynamically recreates the hotkey rows based on the selected hotkey mode.
 * e.g., A single row for 'toggle' mode, or two rows for 'mute' and 'unmute' mode.
 */
function rebuildHotkeyRows() {
    const container = document.getElementById("hotkey-rows");
    container.innerHTML = "";
    const mode = config.hotkey_mode;
    const keys = mode === "toggle" ? ["toggle"] : ["mute", "unmute"];
    for (const key of keys) {
        const label = key.charAt(0).toUpperCase() + key.slice(1);
        const hkCfg = config.hotkey[key] || { vk: 0, name: "None" };
        const currentVk = hkCfg.vk ?? 0;

        // Build options list: COMMON_KEYS + the current recorded key if not already listed
        let options = COMMON_KEYS;
        if (currentVk && !COMMON_KEYS.some(([vk]) => vk === currentVk)) {
            options = [[currentVk, hkCfg.name || vkToName(currentVk)], ...COMMON_KEYS];
        }

        const row = document.createElement("div");
        row.className = "hotkey-row";

        const lbl = document.createElement("label");
        lbl.textContent = label + ":";
        row.appendChild(lbl);

        const select = document.createElement("select");
        select.className = "select-input";
        select.dataset.hkKey = key;
        for (const [vk, name] of options) {
            const opt = document.createElement("option");
            opt.value = vk;
            opt.textContent = name;
            if (currentVk === vk) opt.selected = true;
            select.appendChild(opt);
        }
        row.appendChild(select);

        const recBtn = document.createElement("button");
        recBtn.className = "btn-sm";
        recBtn.dataset.recordKey = key;
        recBtn.id = `rec-${key}`;
        recBtn.textContent = "Record";
        row.appendChild(recBtn);

        const clearBtn = document.createElement("button");
        clearBtn.className = "btn-sm";
        clearBtn.dataset.clearKey = key;
        clearBtn.textContent = "Clear";
        row.appendChild(clearBtn);

        container.appendChild(row);

        row.querySelector(`[data-record-key="${key}"]`).addEventListener("click", async () => {
            startRecording(key);
        });
        row.querySelector(`[data-clear-key="${key}"]`).addEventListener("click", () => {
            if (!config.hotkey[key]) config.hotkey[key] = {};
            config.hotkey[key].vk = 0;
            config.hotkey[key].name = "None";
            rebuildHotkeyRows();
            debouncedSave();
        });
        row.querySelector(`[data-hk-key="${key}"]`).addEventListener("change", e => {
            const vk = parseInt(e.target.value);
            if (!config.hotkey[key]) config.hotkey[key] = {};
            config.hotkey[key].vk = vk;
            config.hotkey[key].name = vkToName(vk);
            rebuildHotkeyRows();
            debouncedSave();
        });
    }
}

/**
 * Instructs the Rust backend to start intercepting keypresses to record a new hotkey.
 * Polls the backend until a new key is successfully recorded.
 * @param {string} key - The action identifier string (e.g. 'toggle', 'mute').
 */
async function startRecording(key) {
    recordingKey = key;
    const btn = document.getElementById(`rec-${key}`);
    btn.textContent = "…";
    btn.classList.add("recording");
    await invoke("start_recording_hotkey");

    // Poll for recorded VK (backend handles keys when other windows are focused)
    recordingPollTimer = setInterval(async () => {
        const vk = await invoke("get_recorded_hotkey");
        if (vk !== null && vk !== undefined) {
            finishRecording(key, vk);
        }
    }, 100);

    // Safety timeout: cancel recording after 10s to prevent keyboard lockup
    setTimeout(() => {
        if (recordingKey === key) cancelRecording(key);
    }, 10000);
}

/**
 * Finishes recording by applying the key and resetting backend state.
 */
function finishRecording(key, vk) {
    if (recordingPollTimer) { clearInterval(recordingPollTimer); recordingPollTimer = null; }
    recordingKey = null;
    const btn = document.getElementById(`rec-${key}`);
    if (btn) { btn.textContent = "Record"; btn.classList.remove("recording"); }
    // Tell backend to exit recording mode (in case JS handled it before the backend)
    invoke("stop_recording_hotkey").catch(() => {});
    config.hotkey[key] = { vk, name: vkToName(vk) };
    rebuildHotkeyRows();
    debouncedSave();
}

/**
 * Cancels recording mode and restores UI without changing the hotkey.
 */
function cancelRecording(key) {
    if (recordingPollTimer) { clearInterval(recordingPollTimer); recordingPollTimer = null; }
    recordingKey = null;
    const btn = document.getElementById(`rec-${key}`);
    if (btn) { btn.textContent = "Record"; btn.classList.remove("recording"); }
    invoke("stop_recording_hotkey").catch(() => {});
}

/**
 * Converts a virtual keycode to a human readable name based on the predefined COMMON_KEYS.
 * @param {number} vk - The virtual keycode
 * @returns {string} The human readable name or hexadecimal string
 */
function vkToName(vk) {
    // Check predefined list first
    const common = COMMON_KEYS.find(([v]) => v === vk);
    if (common) return common[1];
    // Letters A-Z
    if (vk >= 0x41 && vk <= 0x5A) return String.fromCharCode(vk);
    // Digits 0-9
    if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30);
    // Numpad 0-9
    if (vk >= 0x60 && vk <= 0x69) return `Numpad ${vk - 0x60}`;
    // F1-F24
    if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x70 + 1}`;
    // Named keys
    const VK_NAMES = {
        0x08: "Backspace", 0x09: "Tab", 0x0D: "Enter", 0x10: "Shift",
        0x11: "Ctrl", 0x12: "Alt", 0x13: "Pause", 0x14: "Caps Lock",
        0x1B: "Escape", 0x20: "Space", 0x21: "Page Up", 0x22: "Page Down",
        0x23: "End", 0x24: "Home", 0x25: "Left", 0x26: "Up",
        0x27: "Right", 0x28: "Down", 0x2C: "Print Screen", 0x2D: "Insert",
        0x2E: "Delete", 0x5B: "Left Win", 0x5C: "Right Win", 0x5D: "Menu",
        0x6A: "Numpad *", 0x6B: "Numpad +", 0x6D: "Numpad -",
        0x6E: "Numpad .", 0x6F: "Numpad /", 0x90: "Num Lock",
        0x91: "Scroll Lock", 0xA0: "Left Shift", 0xA1: "Right Shift",
        0xA2: "Left Ctrl", 0xA3: "Right Ctrl", 0xA4: "Left Alt",
        0xA5: "Right Alt", 0xBA: ";", 0xBB: "=", 0xBC: ",",
        0xBD: "-", 0xBE: ".", 0xBF: "/", 0xC0: "`",
        0xDB: "[", 0xDC: "\\", 0xDD: "]", 0xDE: "'",
    };
    return VK_NAMES[vk] ?? `Key 0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

// ──────────────────────────────────
//  Event listeners
// ──────────────────────────────────

/**
 * Attaches UI event listeners (clicks, changes, input) to trigger configuration state
 * changes and interact with the rust backend commands.
 */
function setupEventListeners() {
    // Tabs
    document.getElementById("btn-tab-devices").addEventListener("click", () => switchTab('tab-devices'));
    document.getElementById("btn-tab-audio").addEventListener("click", () => switchTab('tab-audio'));
    document.getElementById("btn-tab-hotkeys").addEventListener("click", () => switchTab('tab-hotkeys'));
    document.getElementById("btn-tab-overlay").addEventListener("click", () => switchTab('tab-overlay'));
    document.getElementById("btn-tab-osd").addEventListener("click", () => switchTab('tab-osd'));
    document.getElementById("btn-tab-system").addEventListener("click", () => switchTab('tab-system'));

    // Toggle mute button
    document.getElementById("btn-toggle-mute").addEventListener("click", async () => {
        try {
            const res = await invoke("toggle_mute");
            isMuted = res.is_muted;
            updateMuteUI(isMuted);
        } catch (e) { showDebug("Mute toggle failed: " + e); }
    });

    // Refresh devices
    document.getElementById("btn-refresh-devices").addEventListener("click", async () => {
        devices = (await invoke("get_devices")).map(d => ({ id: d.id, name: d.name }));
        rebuildDeviceSelect();
        rebuildSyncList();
    });

    // Device select change
    document.getElementById("sel-device").addEventListener("change", async e => {
        const id = e.target.value || null;
        await invoke("set_device", { deviceId: id }).catch(err => showDebug("Device switch failed: " + err));
        config.device_id = id;
        rebuildSyncList();
        debouncedSave();
    });

    // Radio buttons – hotkey mode
    document.getElementById("hk-mode-toggle").addEventListener("change", () => {
        config.hotkey_mode = "toggle";
        rebuildHotkeyRows();
        debouncedSave();
    });
    document.getElementById("hk-mode-sep").addEventListener("change", () => {
        config.hotkey_mode = "separate";
        rebuildHotkeyRows();
        debouncedSave();
    });

    // Overlay toggle
    document.getElementById("chk-overlay").addEventListener("change", e => {
        config.persistent_overlay.enabled = e.target.checked;
        updateSubOptions("chk-overlay", "overlay-options");
        debouncedSave();
    });

    // OSD toggle
    document.getElementById("chk-osd").addEventListener("change", e => {
        config.osd.enabled = e.target.checked;
        updateSubOptions("chk-osd", "osd-options");
        debouncedSave();
    });

    // AFK toggle
    document.getElementById("chk-afk").addEventListener("change", e => {
        config.afk.enabled = e.target.checked;
        updateSubOptions("chk-afk", "afk-timeout-row");
        debouncedSave();
    });

    // Sliders
    bindSlider("slider-overlay-scale", "overlay-scale-val", v => config.persistent_overlay.scale = v);
    bindSlider("slider-overlay-opacity", "overlay-opacity-val", v => config.persistent_overlay.opacity = v);
    bindSlider("slider-overlay-sens", "overlay-sens-val", v => config.persistent_overlay.sensitivity = v);
    bindSlider("slider-osd-dur", "osd-dur-val", v => config.osd.duration = v);
    bindSlider("slider-osd-size", "osd-size-val", v => config.osd.size = v);
    bindSlider("slider-osd-opacity", "osd-opacity-val", v => config.osd.opacity = v);
    bindSlider("slider-afk-timeout", "afk-timeout-val", v => config.afk.timeout = v);

    // Selects → config
    document.getElementById("sel-overlay-pos").addEventListener("change", e => {
        config.persistent_overlay.position_mode = e.target.value;
        debouncedSave();
    });
    document.getElementById("sel-overlay-theme").addEventListener("change", e => {
        config.persistent_overlay.theme = e.target.value;
        debouncedSave();
    });
    document.getElementById("sel-osd-pos").addEventListener("change", e => {
        config.osd.position = e.target.value;
        debouncedSave();
    });

    // Checkboxes → config
    document.getElementById("chk-beep").addEventListener("change", e => { config.beep_enabled = e.target.checked; debouncedSave(); });
    document.getElementById("radio-beep").addEventListener("change", () => { 
        config.audio_mode = "beep"; 
        updateAudioModeUI("beep");
        debouncedSave(); 
    });
    document.getElementById("radio-custom").addEventListener("change", () => { 
        config.audio_mode = "custom"; 
        updateAudioModeUI("custom");
        debouncedSave(); 
    });

    // Audio Sliders
    bindSlider("slider-vol-mute", "vol-mute-val", v => {
        if (!config.sound_config) config.sound_config = {};
        if (!config.sound_config.mute) config.sound_config.mute = { file: "", volume: 50 };
        config.sound_config.mute.volume = v;
    });
    bindSlider("slider-vol-unmute", "vol-unmute-val", v => {
        if (!config.sound_config) config.sound_config = {};
        if (!config.sound_config.unmute) config.sound_config.unmute = { file: "", volume: 50 };
        config.sound_config.unmute.volume = v;
    });

    // Beep Inputs
    const bindBeep = (id, key, field) => {
        document.getElementById(id).addEventListener("input", e => {
            if (!config.beep_config) config.beep_config = {};
            if (!config.beep_config[key]) config.beep_config[key] = { freq: 650, duration: 180, count: 1 };
            config.beep_config[key][field] = parseInt(e.target.value) || 0;
            debouncedSave();
        });
    };
    bindBeep("beep-mute-freq", "mute", "freq");
    bindBeep("beep-mute-dur", "mute", "duration");
    bindBeep("beep-mute-count", "mute", "count");
    bindBeep("beep-unmute-freq", "unmute", "freq");
    bindBeep("beep-unmute-dur", "unmute", "duration");
    bindBeep("beep-unmute-count", "unmute", "count");

    // Browsing
    document.getElementById("btn-browse-mute").addEventListener("click", () => pickAudioFile("mute"));
    document.getElementById("btn-browse-unmute").addEventListener("click", () => pickAudioFile("unmute"));

    // Previews
    document.getElementById("btn-preview-mute").addEventListener("click", () => previewAudio("custom", "mute"));
    document.getElementById("btn-preview-unmute").addEventListener("click", () => previewAudio("custom", "unmute"));
    document.getElementById("btn-preview-beep-mute").addEventListener("click", () => previewAudio("beep", "mute"));
    document.getElementById("btn-preview-beep-unmute").addEventListener("click", () => previewAudio("beep", "unmute"));

    document.getElementById("chk-overlay-vu").addEventListener("change", e => { config.persistent_overlay.show_vu = e.target.checked; debouncedSave(); });
    document.getElementById("chk-overlay-locked").addEventListener("change", e => { config.persistent_overlay.locked = e.target.checked; debouncedSave(); });

    // Startup
    document.getElementById("chk-startup").addEventListener("change", async e => {
        await invoke("set_run_on_startup_cmd", { enable: e.target.checked });
    });

    // Save
    document.getElementById("btn-save").addEventListener("click", saveConfig);

    // Sync checkboxes
    document.getElementById("sync-list").addEventListener("change", e => {
        const cb = e.target;
        if (!cb.dataset.syncId) return;
        const id = cb.dataset.syncId;
        if (!config.sync_ids) config.sync_ids = [];
        if (cb.checked) {
            if (!config.sync_ids.includes(id)) config.sync_ids.push(id);
        } else {
            config.sync_ids = config.sync_ids.filter(s => s !== id);
        }
        debouncedSave();
    });

    // Help link
    document.getElementById("link-help").addEventListener("click", e => {
        e.preventDefault();
        invoke("open_url", { url: "https://github.com/madbeat14/MicMuteRS" });
    });
}

// ──────────────────────────────────
//  Save Configuration
// ──────────────────────────────────

/**
 * Pushes the current `config` object state back to the rust backend to be saved to disk
 * and applied to the running application instances. Shows a temporary debug message on success.
 */
async function saveConfig() {
    if (!config) {
        showDebug("Cannot save: Config is NULL! (Initialization failed)", true);
        return;
    }
    try {
        console.log("Saving new config state to backend", config);
        // Pass stringified JSON to bypass Tauri v2 camel/snake auto-conversion bugs
        await invoke("update_config", { payload: JSON.stringify(config) });
        showDebug("Settings saved ✓");
        console.log("Config successfully applied");
    } catch (e) {
        showDebug("Error saving: " + e);
        console.error("FAILED to save config:", e);
    }
}

// ──────────────────────────────────
//  UI DOM Helpers
// ──────────────────────────────────

/**
 * Toggles visibility between Beep and Custom sound controls.
 * @param {string} mode - "beep" or "custom"
 */
function updateAudioModeUI(mode) {
    document.getElementById("audio-beep-controls").style.display = (mode === "beep") ? "block" : "none";
    document.getElementById("audio-custom-controls").style.display = (mode === "custom") ? "block" : "none";
}

/**
 * Triggers a file picker via Tauri's dialog plugin.
 * @param {string} key - "mute" or "unmute"
 */
async function pickAudioFile(key) {
    try {
        const path = await invoke("pick_audio_file");
        if (path) {
            if (!config.sound_config) config.sound_config = {};
            if (!config.sound_config[key]) config.sound_config[key] = { file: "", volume: 50 };
            config.sound_config[key].file = path;
            document.getElementById(`path-${key}`).value = path;
            debouncedSave();
        }
    } catch (e) {
        showDebug("File picking failed: " + e);
    }
}

/**
 * Plays a preview of the sound using current UI parameters without saving first.
 * @param {string} mode - "beep" or "custom"
 * @param {string} key - "mute" or "unmute"
 */
async function previewAudio(mode, key) {
    try {
        const payload = JSON.stringify(config);
        await invoke("preview_audio_feedback", { mode, key, payload });
    } catch (e) {
        showDebug("Preview failed: " + e);
    }
}

/**
 * Updates the text and style of the mute status badge and toggle button.
 * @param {boolean} muted - Active mute state
 */
function updateMuteUI(muted) {
    const badge = document.getElementById("mute-status");
    const btn = document.getElementById("btn-toggle-mute");
    badge.textContent = muted ? "🔇 Muted" : "🎤 Active";
    badge.className = "status-badge " + (muted ? "muted" : "active");
    btn.textContent = muted ? "🔇" : "🎤";
}

/**
 * Updates the width of the Volume Unit (VU) meter bar.
 * Windows IAudioMeterInformation returns very small peak values
 * (typically 0.001-0.05 for speech) so we amplify aggressively.
 * @param {number} peak - The peak audio volume between 0.0 and 1.0
 */
function updateVU(peak) {
    const bar = document.getElementById("vu-bar");
    if (!bar) return;
    // Use the overlay's VU sensitivity threshold to gate noise floor
    const threshold = (config?.persistent_overlay?.sensitivity ?? 5) / 100;
    if (peak < threshold) {
        bar.style.width = "0%";
        return;
    }
    // Map from threshold..1.0 → 0..1, then amplify and compress
    const above = (peak - threshold) / (1 - threshold);
    const scaled = Math.pow(Math.min(1, above * 10), 0.5) * 100;
    bar.style.width = Math.min(100, scaled) + "%";
}

/**
 * Starts an interval timer to constantly poll the backend for the current peak volume
 * level while the settings page is open to animate the VU bar.
 */
function startVuPoll() {
    vuPollTimer = setInterval(async () => {
        try {
            const s = await invoke("get_state");
            updateVU(s.peak_level);
            // DEBUG: show peak value in footer
            const dbg = document.getElementById("debug-msg");
            if (dbg) dbg.textContent = "peak: " + s.peak_level.toFixed(6);
        } catch (e) {
            const dbg = document.getElementById("debug-msg");
            if (dbg) dbg.textContent = "VU error: " + e;
        }
    }, 100);
}



/**
 * Synchronizes an HTML range slider value with its adjacent text label.
 * @param {string} id - HTML ID of the `<input type="range">`
 * @param {number} value - Background config value
 * @param {string} labelId - HTML ID of the `<span>` showing the value
 */
function setSlider(id, value, labelId) {
    const el = document.getElementById(id);
    const lbl = document.getElementById(labelId);
    if (el) el.value = value;
    if (lbl) lbl.textContent = value;
}

/**
 * Sets the active option in an HTML `<select>` element.
 * @param {string} id - HTML ID of the select element
 * @param {string} value - Value to select
 */
function setSelect(id, value) {
    const el = document.getElementById(id);
    if (!el) return;
    [...el.options].forEach(o => { o.selected = o.value === value; });
}

/**
 * Binds an HTML range slider to automatically update its text label and invoke 
 * a callback whenever the user scrubs the slider thumb.
 * @param {string} sliderId - HTML ID of the `<input type="range">`
 * @param {string} labelId - HTML ID of the `<span>` to update
 * @param {function} onValue - Callback invoked with the integer value when changed
 */
function bindSlider(sliderId, labelId, onValue) {
    const el = document.getElementById(sliderId);
    const lbl = document.getElementById(labelId);
    if (!el) return;
    el.addEventListener("input", () => {
        const v = parseInt(el.value);
        if (lbl) lbl.textContent = v;
        onValue(v);
        debouncedSave();
    });
}

/**
 * Visually enables or disables a block of options depending on a master checkbox state.
 * @param {string} checkId - HTML ID of the master checkbox
 * @param {string} optionsId - HTML ID of the container element representing the children options
 */
function updateSubOptions(checkId, optionsId) {
    const chk = document.getElementById(checkId);
    const opts = document.getElementById(optionsId);
    if (!chk || !opts) return;
    opts.style.opacity = chk.checked ? "1" : "0.4";
    opts.style.pointerEvents = chk.checked ? "auto" : "none";
}

// ──────────────────────────────────
//  Hotkey passthrough for focused WebView
// ──────────────────────────────────

/**
 * Maps a JS KeyboardEvent.code / key to a Windows virtual-key code.
 * This is needed because WebView2 consumes certain keys (especially media keys)
 * before they reach the global WH_KEYBOARD_LL hook.
 */
const JS_KEY_TO_VK = {
    'MediaPlayPause': 0xB3, 'MediaTrackNext': 0xB0, 'MediaTrackPrevious': 0xB1,
    'MediaStop': 0xB2, 'AudioVolumeMute': 0xAD, 'AudioVolumeDown': 0xAE,
    'AudioVolumeUp': 0xAF, 'F1': 0x70, 'F2': 0x71, 'F3': 0x72, 'F4': 0x73,
    'F5': 0x74, 'F6': 0x75, 'F7': 0x76, 'F8': 0x77, 'F9': 0x78, 'F10': 0x79,
    'F11': 0x7A, 'F12': 0x7B, 'Space': 0x20, 'Enter': 0x0D, 'Backspace': 0x08,
    'Tab': 0x09, 'Escape': 0x1B, 'CapsLock': 0x14, 'Pause': 0x13,
};

/**
 * Resolves a Windows VK code from a JS KeyboardEvent.
 */
function jsEventToVK(e) {
    let vk = JS_KEY_TO_VK[e.key] || JS_KEY_TO_VK[e.code] || 0;
    // For letter, digit, and other standard keys, keyCode maps to Windows VK
    if (!vk && e.keyCode >= 0x08) vk = e.keyCode;
    return vk;
}

/**
 * Installs a capture-phase keydown listener that checks if the pressed key
 * matches any configured hotkey. If so, it invokes the corresponding mute
 * command directly, bypassing the LL hook that WebView2 may have blocked.
 */
function setupHotkeyPassthrough() {
    document.addEventListener('keydown', async (e) => {
        if (!config) return;
        // During recording, feed the key to the backend directly
        // since WebView2 may consume it before the LL hook sees it
        if (recordingKey) {
            const vk = jsEventToVK(e);
            if (vk) {
                e.preventDefault();
                e.stopPropagation();
                finishRecording(recordingKey, vk);
            }
            return;
        }

        const vk = jsEventToVK(e);
        if (!vk) return;

        const mode = config.hotkey_mode;
        let matched = false;

        if (mode === 'toggle') {
            const toggleVk = config.hotkey?.toggle?.vk || 0;
            if (toggleVk && vk === toggleVk) {
                matched = true;
                try {
                    const res = await invoke("toggle_mute");
                    isMuted = res.is_muted;
                    updateMuteUI(isMuted);
                } catch (_) {}
            }
        } else {
            const muteVk = config.hotkey?.mute?.vk || 0;
            const unmuteVk = config.hotkey?.unmute?.vk || 0;
            if (muteVk && muteVk === unmuteVk && vk === muteVk) {
                matched = true;
                try {
                    const res = await invoke("toggle_mute");
                    isMuted = res.is_muted;
                    updateMuteUI(isMuted);
                } catch (_) {}
            } else if (muteVk && vk === muteVk) {
                matched = true;
                try {
                    const res = await invoke("set_mute", { muted: true });
                    isMuted = res.is_muted;
                    updateMuteUI(isMuted);
                } catch (_) {}
            } else if (unmuteVk && vk === unmuteVk) {
                matched = true;
                try {
                    const res = await invoke("set_mute", { muted: false });
                    isMuted = res.is_muted;
                    updateMuteUI(isMuted);
                } catch (_) {}
            }
        }

        if (matched) {
            e.preventDefault();
            e.stopPropagation();
        }
    }, true);
}

/**
 * Switches the active tab in the settings menu.
 * @param {string} tabId - HTML ID of the tab pane to show
 */
function switchTab(tabId) {
    // Hide all tab panes
    document.querySelectorAll('.tab-pane').forEach(pane => {
        pane.classList.remove('active');
    });
    // Remove active class from all tab buttons
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.remove('active');
    });

    // Show target tab
    document.getElementById(tabId).classList.add('active');

    // Set matching button to active
    const btnId = 'btn-' + tabId;
    const btn = document.getElementById(btnId);
    if (btn) btn.classList.add('active');

}

/**
 * Measures all tab panes to find the tallest, then resizes the window once
 * so every tab fits without dead space or per-tab resizing.
 */
async function autoFitWindow() {
    await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));

    const { getCurrentWindow } = window.__TAURI__.window;
    const win = getCurrentWindow();

    // Measure minimum width for tabs in one row
    const tabsNav = document.querySelector('.tabs-nav');
    let tabsWidth = 0;
    tabsNav.querySelectorAll('.tab-btn').forEach(btn => {
        tabsWidth += btn.offsetWidth;
    });
    const tabCount = tabsNav.querySelectorAll('.tab-btn').length;
    tabsWidth += (tabCount - 1) * 4 + 28;

    // Temporarily show all tab panes to measure their natural heights
    const panes = document.querySelectorAll('.tab-pane');
    const content = document.querySelector('.settings-content');
    const prevFlex = content.style.flex;
    const prevOverflow = content.style.overflow;
    content.style.flex = '0 0 auto';
    content.style.overflow = 'visible';

    // Save which pane was active
    const activePane = document.querySelector('.tab-pane.active');

    let maxPaneH = 0;
    panes.forEach(pane => {
        const wasActive = pane.classList.contains('active');
        if (!wasActive) pane.classList.add('active');
        const h = pane.scrollHeight;
        if (h > maxPaneH) maxPaneH = h;
        if (!wasActive) pane.classList.remove('active');
    });

    // Restore
    content.style.flex = prevFlex;
    content.style.overflow = prevOverflow;

    const header = document.querySelector('.app-header');
    const footer = document.querySelector('.app-footer');
    // content padding: 12px top + 12px bottom
    const totalHeight = header.offsetHeight + tabsNav.offsetHeight + maxPaneH + 24 + footer.offsetHeight;

    const desiredW = Math.max(tabsWidth, 480);
    const screenH = window.screen.availHeight;
    const finalH = Math.min(totalHeight, Math.floor(screenH * 0.9));

    const { LogicalSize } = window.__TAURI__.window;
    await win.setSize(new LogicalSize(desiredW, finalH));
}

/**
 * Prints a temporary message to the debug status label at the bottom of the window.
 * @param {string} msg - Resulting message
 */
function showDebug(msg) {
    const el = document.getElementById("debug-msg");
    if (el) el.textContent = msg;
    setTimeout(() => { if (el) el.textContent = ""; }, 3000);
}

// ──────────────────────────────────
//  Start
// ──────────────────────────────────
window.addEventListener("DOMContentLoaded", init);
window.switchTab = switchTab;
