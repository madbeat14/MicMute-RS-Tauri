// MicMuteRs – Settings Page Logic
// Uses window.__TAURI__ provided by Tauri

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ──────────────────────────────────
//  State
// ──────────────────────────────────
let config = null;
let devices = [];
let isMuted = false;
let recordingKey = null;
let recordingPollTimer = null;
let vuPollTimer = null;

let saveTimeout = null;
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
//  Init
// ──────────────────────────────────
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
//  Apply config → UI
// ──────────────────────────────────
function applyConfigToUI() {
    if (!config) return;

    // Device
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
//  Device select
// ──────────────────────────────────
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
//  Hotkey rows
// ──────────────────────────────────
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

function vkToName(vk) {
    return COMMON_KEYS.find(([v]) => v === vk)?.[1] ?? `VK_0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

// ──────────────────────────────────
//  Event listeners
// ──────────────────────────────────
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
//  Save
// ──────────────────────────────────
async function saveConfig() {
    try {
        await invoke("update_config", { newConfig: config });
        showDebug("Settings saved ✓");
    } catch (e) {
        showDebug("Error saving: " + e);
    }
}

// ──────────────────────────────────
//  UI helpers
// ──────────────────────────────────
function updateMuteUI(muted) {
    const badge = document.getElementById("mute-status");
    const btn = document.getElementById("btn-toggle-mute");
    badge.textContent = muted ? "🔇 Muted" : "🎤 Active";
    badge.className = "status-badge " + (muted ? "muted" : "active");
    btn.textContent = muted ? "🔇" : "🎤";
}

function updateVU(peak) {
    const bar = document.getElementById("vu-bar");
    if (bar) bar.style.width = Math.min(100, peak * 300) + "%";
}

function startVuPoll() {
    vuPollTimer = setInterval(async () => {
        try {
            const s = await invoke("get_state");
            updateVU(s.peak_level);
        } catch (_) { }
    }, 100);
}

function setSlider(id, value, labelId) {
    const el = document.getElementById(id);
    const lbl = document.getElementById(labelId);
    if (el) el.value = value;
    if (lbl) lbl.textContent = value;
}

function setSelect(id, value) {
    const el = document.getElementById(id);
    if (!el) return;
    [...el.options].forEach(o => { o.selected = o.value === value; });
}

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

function updateSubOptions(checkId, optionsId) {
    const chk = document.getElementById(checkId);
    const opts = document.getElementById(optionsId);
    if (!chk || !opts) return;
    opts.style.opacity = chk.checked ? "1" : "0.4";
    opts.style.pointerEvents = chk.checked ? "auto" : "none";
}

function toggleSection(sectionId) {
    document.getElementById(sectionId).classList.toggle("collapsed");
}

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
