// MicMuteRs – Settings Page Logic
// Uses window.__TAURI__ provided by Tauri to interact with the rust backend.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ──────────────────────────────────
//  State variables
// ──────────────────────────────────
// Exposed globally so other split scripts can access them
window.config = null;
window.devices = [];
window.isMuted = false;
window.vuPollTimer = null;

let saveTimeout = null;
window.isSaving = false;

function debouncedSave() {
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(saveConfig, 300);
}
window.debouncedSave = debouncedSave; // export for other modules

// ──────────────────────────────────
//  Initialization
// ──────────────────────────────────
async function init() {
    try { window.config = await invoke("get_config"); } catch (e) { console.error(e); }
    try { window.isMuted = (await invoke("get_state")).is_muted; } catch (e) { console.error(e); }
    try { 
        window.devices = (await invoke("get_cached_devices")).map(d => ({ id: d.id, name: d.name }));
    } catch (e) { window.devices = []; }

    applyConfigToUI();
    if (typeof startVuPoll === 'function') startVuPoll();
    setupEventListeners();
    if (typeof setupHotkeyPassthrough === 'function') setupHotkeyPassthrough();
    
    await listen("state-update", e => {
        window.isMuted = e.payload.is_muted;
        if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
        if (typeof updateVU === 'function') updateVU(e.payload.peak_level);
    });

    await listen("config-update", e => {
        if (window.isSaving) return;
        window.config = e.payload.config;
        applyConfigToUI();
    });

    if (typeof autoFitWindow === 'function') autoFitWindow();
}

// ──────────────────────────────────
//  Configuration to UI synchronization
// ──────────────────────────────────
function applyConfigToUI() {
    if (!window.config) return;

    rebuildDeviceSelect();
    rebuildSyncList();

    document.getElementById("chk-beep").checked = window.config.beep_enabled;
    const mode = window.config.audio_mode || "beep";
    document.getElementById("radio-beep").checked = mode === "beep";
    document.getElementById("radio-custom").checked = mode === "custom";
    if (typeof updateAudioModeUI === 'function') updateAudioModeUI(mode);

    document.getElementById("path-mute").value = window.config.sound_config?.mute?.file || "";
    document.getElementById("path-unmute").value = window.config.sound_config?.unmute?.file || "";

    setSlider("slider-vol-mute", window.config.sound_config?.mute?.volume || 50, "vol-mute-val");
    setSlider("slider-vol-unmute", window.config.sound_config?.unmute?.volume || 50, "vol-unmute-val");

    const bm = window.config.beep_config?.mute || { freq: 650, duration: 180, count: 2 };
    const bu = window.config.beep_config?.unmute || { freq: 700, duration: 200, count: 1 };
    document.getElementById("beep-mute-freq").value = bm.freq;
    document.getElementById("beep-mute-dur").value = bm.duration;
    document.getElementById("beep-mute-count").value = bm.count;
    document.getElementById("beep-unmute-freq").value = bu.freq;
    document.getElementById("beep-unmute-dur").value = bu.duration;
    document.getElementById("beep-unmute-count").value = bu.count;

    document.getElementById("hk-mode-toggle").checked = window.config.hotkey_mode === "toggle";
    document.getElementById("hk-mode-sep").checked = window.config.hotkey_mode === "separate";
    if (typeof rebuildHotkeyRows === 'function') rebuildHotkeyRows();

    const ol = window.config.persistent_overlay;
    document.getElementById("chk-overlay").checked = ol.enabled;
    document.getElementById("chk-overlay-vu").checked = ol.show_vu;
    document.getElementById("chk-overlay-locked").checked = ol.locked;
    setSelect("sel-overlay-pos", ol.position_mode);
    setSelect("sel-overlay-theme", ol.theme);
    setSlider("slider-overlay-scale", ol.scale, "overlay-scale-val");
    setSlider("slider-overlay-opacity", ol.opacity, "overlay-opacity-val");
    setSlider("slider-overlay-sens", ol.sensitivity, "overlay-sens-val");
    updateSubOptions("chk-overlay", "overlay-options");

    const osd = window.config.osd;
    document.getElementById("chk-osd").checked = osd.enabled;
    setSlider("slider-osd-dur", osd.duration, "osd-dur-val");
    setSlider("slider-osd-size", osd.size, "osd-size-val");
    setSlider("slider-osd-opacity", osd.opacity, "osd-opacity-val");
    setSelect("sel-osd-pos", osd.position);
    updateSubOptions("chk-osd", "osd-options");

    invoke("get_run_on_startup_cmd").then(b => {
        document.getElementById("chk-startup").checked = b;
    });
    document.getElementById("chk-afk").checked = window.config.afk.enabled;
    setSlider("slider-afk-timeout", window.config.afk.timeout, "afk-timeout-val");
    updateSubOptions("chk-afk", "afk-timeout-row");

    if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
}

// ──────────────────────────────────
//  Device Selection Logic
// ──────────────────────────────────
function rebuildDeviceSelect() {
    const sel = document.getElementById("sel-device");
    sel.textContent = "";
    const defaultOpt = document.createElement("option");
    defaultOpt.value = "";
    defaultOpt.textContent = "Default Windows Device";
    sel.appendChild(defaultOpt);
    for (const d of window.devices) {
        const opt = document.createElement("option");
        opt.value = d.id;
        opt.textContent = d.name;
        if (window.config.device_id === d.id) opt.selected = true;
        sel.appendChild(opt);
    }
}

function rebuildSyncList() {
    const container = document.getElementById("sync-list");
    container.replaceChildren();
    const primaryId = window.config.device_id;
    for (const d of window.devices) {
        if (d.id === primaryId) continue;
        const isSynced = (window.config.sync_ids || []).includes(d.id);
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
//  Event listeners
// ──────────────────────────────────
function setupEventListeners() {
    bindTabListeners();
    bindAudioListeners();
    bindHotkeyListeners();
    bindOverlayListeners();
    bindOsdListeners();
    bindSystemListeners();

    // Toggle mute button
    document.getElementById("btn-toggle-mute").addEventListener("click", async () => {
        try {
            const res = await invoke("toggle_mute");
            window.isMuted = res.is_muted;
            if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
        } catch (e) { showDebug("Mute toggle failed: " + e); }
    });

    // Refresh devices
    document.getElementById("btn-refresh-devices").addEventListener("click", async () => {
        window.devices = (await invoke("get_devices")).map(d => ({ id: d.id, name: d.name }));
        rebuildDeviceSelect();
        rebuildSyncList();
    });

    // Device select change
    document.getElementById("sel-device").addEventListener("change", async e => {
        const id = e.target.value || null;
        await invoke("set_device", { deviceId: id }).catch(err => showDebug("Device switch failed: " + err));
        window.config.device_id = id;
        rebuildSyncList();
        debouncedSave();
    });

    // Save
    document.getElementById("btn-save").addEventListener("click", saveConfig);

    // Sync checkboxes
    document.getElementById("sync-list").addEventListener("change", e => {
        const cb = e.target;
        if (!cb.dataset.syncId) return;
        const id = cb.dataset.syncId;
        if (!window.config.sync_ids) window.config.sync_ids = [];
        if (cb.checked) {
            if (!window.config.sync_ids.includes(id)) window.config.sync_ids.push(id);
        } else {
            window.config.sync_ids = window.config.sync_ids.filter(s => s !== id);
        }
        debouncedSave();
    });

    // Help link
    document.getElementById("link-help").addEventListener("click", e => {
        e.preventDefault();
        invoke("open_url", { url: "https://github.com/madbeat14/MicMuteRS" });
    });
}

function bindTabListeners() {
    document.getElementById("btn-tab-devices").addEventListener("click", () => switchTab('tab-devices'));
    document.getElementById("btn-tab-audio").addEventListener("click", () => switchTab('tab-audio'));
    document.getElementById("btn-tab-hotkeys").addEventListener("click", () => switchTab('tab-hotkeys'));
    document.getElementById("btn-tab-overlay").addEventListener("click", () => switchTab('tab-overlay'));
    document.getElementById("btn-tab-osd").addEventListener("click", () => switchTab('tab-osd'));
    document.getElementById("btn-tab-system").addEventListener("click", () => switchTab('tab-system'));
}

function bindAudioListeners() {
    document.getElementById("chk-beep").addEventListener("change", e => { window.config.beep_enabled = e.target.checked; debouncedSave(); });
    document.getElementById("radio-beep").addEventListener("change", () => { 
        window.config.audio_mode = "beep"; 
        if (typeof updateAudioModeUI === 'function') updateAudioModeUI("beep");
        debouncedSave(); 
    });
    document.getElementById("radio-custom").addEventListener("change", () => { 
        window.config.audio_mode = "custom"; 
        if (typeof updateAudioModeUI === 'function') updateAudioModeUI("custom");
        debouncedSave(); 
    });

    bindSlider("slider-vol-mute", "vol-mute-val", v => {
        if (!window.config.sound_config) window.config.sound_config = {};
        if (!window.config.sound_config.mute) window.config.sound_config.mute = { file: "", volume: 50 };
        window.config.sound_config.mute.volume = v;
    });
    bindSlider("slider-vol-unmute", "vol-unmute-val", v => {
        if (!window.config.sound_config) window.config.sound_config = {};
        if (!window.config.sound_config.unmute) window.config.sound_config.unmute = { file: "", volume: 50 };
        window.config.sound_config.unmute.volume = v;
    });

    const bindBeep = (id, key, field) => {
        document.getElementById(id).addEventListener("input", e => {
            if (!window.config.beep_config) window.config.beep_config = {};
            if (!window.config.beep_config[key]) window.config.beep_config[key] = { freq: 650, duration: 180, count: 1 };
            window.config.beep_config[key][field] = parseInt(e.target.value) || 0;
            debouncedSave();
        });
    };
    bindBeep("beep-mute-freq", "mute", "freq");
    bindBeep("beep-mute-dur", "mute", "duration");
    bindBeep("beep-mute-count", "mute", "count");
    bindBeep("beep-unmute-freq", "unmute", "freq");
    bindBeep("beep-unmute-dur", "unmute", "duration");
    bindBeep("beep-unmute-count", "unmute", "count");

    if (typeof pickAudioFile === 'function') {
        document.getElementById("btn-browse-mute").addEventListener("click", () => pickAudioFile("mute"));
        document.getElementById("btn-browse-unmute").addEventListener("click", () => pickAudioFile("unmute"));
    }
    
    if (typeof previewAudio === 'function') {
        document.getElementById("btn-preview-mute").addEventListener("click", () => previewAudio("custom", "mute"));
        document.getElementById("btn-preview-unmute").addEventListener("click", () => previewAudio("custom", "unmute"));
        document.getElementById("btn-preview-beep-mute").addEventListener("click", () => previewAudio("beep", "mute"));
        document.getElementById("btn-preview-beep-unmute").addEventListener("click", () => previewAudio("beep", "unmute"));
    }
}

function bindHotkeyListeners() {
    document.getElementById("hk-mode-toggle").addEventListener("change", () => {
        window.config.hotkey_mode = "toggle";
        if (typeof rebuildHotkeyRows === 'function') rebuildHotkeyRows();
        debouncedSave();
    });
    document.getElementById("hk-mode-sep").addEventListener("change", () => {
        window.config.hotkey_mode = "separate";
        if (typeof rebuildHotkeyRows === 'function') rebuildHotkeyRows();
        debouncedSave();
    });
}

function bindOverlayListeners() {
    document.getElementById("chk-overlay").addEventListener("change", e => {
        window.config.persistent_overlay.enabled = e.target.checked;
        updateSubOptions("chk-overlay", "overlay-options");
        debouncedSave();
    });
    document.getElementById("chk-overlay-vu").addEventListener("change", e => { window.config.persistent_overlay.show_vu = e.target.checked; debouncedSave(); });
    document.getElementById("chk-overlay-locked").addEventListener("change", e => { window.config.persistent_overlay.locked = e.target.checked; debouncedSave(); });

    bindSlider("slider-overlay-scale", "overlay-scale-val", v => window.config.persistent_overlay.scale = v);
    bindSlider("slider-overlay-opacity", "overlay-opacity-val", v => window.config.persistent_overlay.opacity = v);
    bindSlider("slider-overlay-sens", "overlay-sens-val", v => window.config.persistent_overlay.sensitivity = v);

    document.getElementById("sel-overlay-pos").addEventListener("change", e => {
        window.config.persistent_overlay.position_mode = e.target.value;
        debouncedSave();
    });
    document.getElementById("sel-overlay-theme").addEventListener("change", e => {
        window.config.persistent_overlay.theme = e.target.value;
        debouncedSave();
    });
}

function bindOsdListeners() {
    document.getElementById("chk-osd").addEventListener("change", e => {
        window.config.osd.enabled = e.target.checked;
        updateSubOptions("chk-osd", "osd-options");
        debouncedSave();
    });
    bindSlider("slider-osd-dur", "osd-dur-val", v => window.config.osd.duration = v);
    bindSlider("slider-osd-size", "osd-size-val", v => window.config.osd.size = v);
    bindSlider("slider-osd-opacity", "osd-opacity-val", v => window.config.osd.opacity = v);
    document.getElementById("sel-osd-pos").addEventListener("change", e => {
        window.config.osd.position = e.target.value;
        debouncedSave();
    });
}

function bindSystemListeners() {
    document.getElementById("chk-afk").addEventListener("change", e => {
        window.config.afk.enabled = e.target.checked;
        updateSubOptions("chk-afk", "afk-timeout-row");
        debouncedSave();
    });
    bindSlider("slider-afk-timeout", "afk-timeout-val", v => window.config.afk.timeout = v);

    document.getElementById("chk-startup").addEventListener("change", async e => {
        await invoke("set_run_on_startup_cmd", { enable: e.target.checked });
    });
}

// ──────────────────────────────────
//  Save Configuration
// ──────────────────────────────────
async function saveConfig() {
    if (!window.config) {
        showDebug("Cannot save: Config is NULL!", true);
        return;
    }
    try {
        window.isSaving = true;
        await invoke("update_config", { payload: JSON.stringify(window.config) });
        window.isSaving = false;
        showDebug("Settings saved");
    } catch (e) {
        window.isSaving = false;
        showDebug("Error saving: " + e);
        console.error(e);
    }
}

// ──────────────────────────────────
//  Start
// ──────────────────────────────────
window.addEventListener("DOMContentLoaded", init);
