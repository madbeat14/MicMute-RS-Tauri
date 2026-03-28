// Hotkey UI logic

const COMMON_KEYS = [
    [0xB3, "Media Play/Pause"], [0x70, "F1"], [0x71, "F2"],
    [0x72, "F3"], [0x73, "F4"], [0x74, "F5"], [0x75, "F6"],
    [0x76, "F7"], [0x77, "F8"], [0x78, "F9"], [0x79, "F10"],
    [0x7A, "F11"], [0x7B, "F12"], [0x20, "Space"], [0x0D, "Enter"],
    [0xAD, "Volume Mute"], [0xAE, "Volume Down"], [0xAF, "Volume Up"],
];

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

function vkToName(vk) {
    const common = COMMON_KEYS.find(([v]) => v === vk);
    if (common) return common[1];
    if (vk >= 0x41 && vk <= 0x5A) return String.fromCharCode(vk);
    if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30);
    if (vk >= 0x60 && vk <= 0x69) return `Numpad ${vk - 0x60}`;
    if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x70 + 1}`;
    return VK_NAMES[vk] ?? `Key 0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

function rebuildHotkeyRows() {
    const container = document.getElementById("hotkey-rows");
    container.replaceChildren();
    const mode = config.hotkey_mode;
    const keys = mode === "toggle" ? ["toggle"] : ["mute", "unmute"];
    for (const key of keys) {
        const label = key.charAt(0).toUpperCase() + key.slice(1);
        const hkCfg = config.hotkey[key] || { vk: 0, name: "None" };
        const currentVk = hkCfg.vk ?? 0;

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
            if (typeof debouncedSave === 'function') debouncedSave();
        });
        row.querySelector(`[data-hk-key="${key}"]`).addEventListener("change", e => {
            const vk = parseInt(e.target.value);
            if (!config.hotkey[key]) config.hotkey[key] = {};
            config.hotkey[key].vk = vk;
            config.hotkey[key].name = vkToName(vk);
            rebuildHotkeyRows();
            if (typeof debouncedSave === 'function') debouncedSave();
        });
    }
}

let recordingKey = null;
let recordingPollTimer = null;
let recordingSafetyTimer = null;

async function startRecording(key) {
    recordingKey = key;
    const btn = document.getElementById(`rec-${key}`);
    btn.textContent = "…";
    btn.classList.add("recording");
    await window.__TAURI__.core.invoke("start_recording_hotkey");

    recordingPollTimer = setInterval(async () => {
        const vk = await window.__TAURI__.core.invoke("get_recorded_hotkey");
        if (vk !== null && vk !== undefined) {
            finishRecording(key, vk);
        }
    }, 100);

    if (recordingSafetyTimer) clearTimeout(recordingSafetyTimer);
    recordingSafetyTimer = setTimeout(() => {
        if (recordingKey === key) cancelRecording(key);
        recordingSafetyTimer = null;
    }, 10000);
}

function finishRecording(key, vk) {
    if (recordingPollTimer) { clearInterval(recordingPollTimer); recordingPollTimer = null; }
    if (recordingSafetyTimer) { clearTimeout(recordingSafetyTimer); recordingSafetyTimer = null; }
    recordingKey = null;
    const btn = document.getElementById(`rec-${key}`);
    if (btn) { btn.textContent = "Record"; btn.classList.remove("recording"); }
    window.__TAURI__.core.invoke("stop_recording_hotkey").catch(() => {});
    config.hotkey[key] = { vk, name: vkToName(vk) };
    rebuildHotkeyRows();
    if (typeof debouncedSave === 'function') debouncedSave();
}

function cancelRecording(key) {
    if (recordingPollTimer) { clearInterval(recordingPollTimer); recordingPollTimer = null; }
    if (recordingSafetyTimer) { clearTimeout(recordingSafetyTimer); recordingSafetyTimer = null; }
    recordingKey = null;
    const btn = document.getElementById(`rec-${key}`);
    if (btn) { btn.textContent = "Record"; btn.classList.remove("recording"); }
    window.__TAURI__.core.invoke("stop_recording_hotkey").catch(() => {});
}

const JS_KEY_TO_VK = {
    'MediaPlayPause': 0xB3, 'MediaTrackNext': 0xB0, 'MediaTrackPrevious': 0xB1,
    'MediaStop': 0xB2, 'AudioVolumeMute': 0xAD, 'AudioVolumeDown': 0xAE,
    'AudioVolumeUp': 0xAF, 'F1': 0x70, 'F2': 0x71, 'F3': 0x72, 'F4': 0x73,
    'F5': 0x74, 'F6': 0x75, 'F7': 0x76, 'F8': 0x77, 'F9': 0x78, 'F10': 0x79,
    'F11': 0x7A, 'F12': 0x7B, 'Space': 0x20, 'Enter': 0x0D, 'Backspace': 0x08,
    'Tab': 0x09, 'Escape': 0x1B, 'CapsLock': 0x14, 'Pause': 0x13,
};

function jsEventToVK(e) {
    let vk = JS_KEY_TO_VK[e.key] || JS_KEY_TO_VK[e.code] || 0;
    if (!vk && e.keyCode >= 0x08) vk = e.keyCode;
    return vk;
}

function setupHotkeyPassthrough() {
    document.addEventListener('keydown', async (e) => {
        if (!window.config) return;
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

        const mode = window.config.hotkey_mode;
        let matched = false;

        if (mode === 'toggle') {
            const toggleVk = window.config.hotkey?.toggle?.vk || 0;
            if (toggleVk && vk === toggleVk) {
                matched = true;
                try {
                    const res = await window.__TAURI__.core.invoke("toggle_mute");
                    window.isMuted = res.is_muted;
                    if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
                } catch (_) {}
            }
        } else {
            const muteVk = window.config.hotkey?.mute?.vk || 0;
            const unmuteVk = window.config.hotkey?.unmute?.vk || 0;
            if (muteVk && muteVk === unmuteVk && vk === muteVk) {
                matched = true;
                try {
                    const res = await window.__TAURI__.core.invoke("toggle_mute");
                    window.isMuted = res.is_muted;
                    if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
                } catch (_) {}
            } else if (muteVk && vk === muteVk) {
                matched = true;
                try {
                    const res = await window.__TAURI__.core.invoke("set_mute", { muted: true });
                    window.isMuted = res.is_muted;
                    if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
                } catch (_) {}
            } else if (unmuteVk && vk === unmuteVk) {
                matched = true;
                try {
                    const res = await window.__TAURI__.core.invoke("set_mute", { muted: false });
                    window.isMuted = res.is_muted;
                    if (typeof updateMuteUI === 'function') updateMuteUI(window.isMuted);
                } catch (_) {}
            }
        }

        if (matched) {
            e.preventDefault();
            e.stopPropagation();
        }
    }, true);
}
