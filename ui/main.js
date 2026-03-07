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
        isMuted = (await invoke("get_state")).is_muted;
        devices = (await invoke("get_devices")).map(d => ({ id: d.id, name: d.name }));
    } catch (e) {
        console.error("init error:", e);
    }
    applyConfigToUI();
    startVuPoll();
    setupEventListeners();
    await listen("state-update", e => {
        isMuted = e.payload.is_muted;
        updateMuteUI(isMuted);
        updateVU(e.payload.peak_level);
    });
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
    document.getElementById("radio-beep").checked = config.audio_mode === "beep";
    document.getElementById("radio-custom").checked = config.audio_mode === "custom";

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
        label.innerHTML = `<input type="checkbox" data-sync-id="${d.id}" ${isSynced ? "checked" : ""} /> ${d.name}`;
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

        const row = document.createElement("div");
        row.className = "hotkey-row";
        row.innerHTML = `
      <label>${label}:</label>
      <select class="select-input" data-hk-key="${key}">
        ${COMMON_KEYS.map(([vk, name]) =>
            `<option value="${vk}" ${currentVk === vk ? "selected" : ""}>${name}</option>`
        ).join("")}
      </select>
      <button class="btn-sm" data-record-key="${key}" id="rec-${key}">Record</button>
      <button class="btn-sm" data-clear-key="${key}">Clear</button>
    `;
        container.appendChild(row);

        row.querySelector(`[data-record-key="${key}"]`).addEventListener("click", async () => {
            startRecording(key);
        });
        row.querySelector(`[data-clear-key="${key}"]`).addEventListener("click", () => {
            if (!config.hotkey[key]) config.hotkey[key] = {};
            config.hotkey[key].vk = 0;
            config.hotkey[key].name = "None";
            rebuildHotkeyRows();
        });
        row.querySelector(`[data-hk-key="${key}"]`).addEventListener("change", e => {
            const vk = parseInt(e.target.value);
            const name = COMMON_KEYS.find(([v]) => v === vk)?.[1] ?? `VK_0x${vk.toString(16).toUpperCase()}`;
            if (!config.hotkey[key]) config.hotkey[key] = {};
            config.hotkey[key].vk = vk;
            config.hotkey[key].name = name;
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

    // Poll for recorded VK
    recordingPollTimer = setInterval(async () => {
        const vk = await invoke("get_recorded_hotkey");
        if (vk !== null && vk !== undefined) {
            clearInterval(recordingPollTimer);
            recordingKey = null;
            btn.textContent = "Record";
            btn.classList.remove("recording");
            config.hotkey[key] = { vk, name: vkToName(vk) };
            rebuildHotkeyRows();
            debouncedSave();
        }
    }, 100);
}

/**
 * Converts a virtual keycode to a human readable name based on the predefined COMMON_KEYS.
 * @param {number} vk - The virtual keycode
 * @returns {string} The human readable name or hexadecimal string
 */
function vkToName(vk) {
    return COMMON_KEYS.find(([v]) => v === vk)?.[1] ?? `VK_0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

// ──────────────────────────────────
//  Event listeners
// ──────────────────────────────────

/**
 * Attaches UI event listeners (clicks, changes, input) to trigger configuration state
 * changes and interact with the rust backend commands.
 */
function setupEventListeners() {
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
    document.getElementById("radio-beep").addEventListener("change", () => { config.audio_mode = "beep"; debouncedSave(); });
    document.getElementById("radio-custom").addEventListener("change", () => { config.audio_mode = "custom"; debouncedSave(); });
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
    try {
        await invoke("update_config", { newConfig: config });
        showDebug("Settings saved ✓");
    } catch (e) {
        showDebug("Error saving: " + e);
    }
}

// ──────────────────────────────────
//  UI DOM Helpers
// ──────────────────────────────────

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
 * @param {number} peak - The peak audio volume between 0.0 and 1.0
 */
function updateVU(peak) {
    const bar = document.getElementById("vu-bar");
    if (bar) bar.style.width = Math.min(100, peak * 300) + "%";
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
        } catch (_) { }
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

/**
 * Toggles the expanded/collapsed CSS class state of an accordion section.
 * @param {string} sectionId - HTML ID of the section wrapper
 */
function toggleSection(sectionId) {
    document.getElementById(sectionId).classList.toggle("collapsed");
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
window.toggleSection = toggleSection;
