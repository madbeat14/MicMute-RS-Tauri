// Overlay window logic
// Static windows "overlay" and "overlay-2" are mapped to monitors by index.
// The backend assigns each window a monitor config key via get_window_monitor_key.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let config = null;
let isMuted = false;
let vuPollTimer = null;
let isDragging = false;
let dragTimeout = null;
let topmostIntervalId = null;
let dragMousedownHandler = null;
let dragMouseupHandler = null;
let dragMousemoveHandler = null;

let unlistenState = null;
let unlistenConfig = null;

const { getCurrentWindow } = window.__TAURI__.window;
const _selfWin = getCurrentWindow();
const _label = _selfWin.label;

// Monitor config key — resolved from backend during init.
let monitorKey = null;

/**
 * Returns the OverlayConfig entry for this window's monitor key, with
 * fallback to "primary" and then to the first available entry.
 */
function getMyConfig() {
    if (!config || !config.persistent_overlay) return null;
    return config.persistent_overlay[monitorKey]
        || config.persistent_overlay['primary']
        || Object.values(config.persistent_overlay)[0]
        || null;
}

async function init() {
    // Query the backend for which monitor config key this window is assigned to.
    try {
        monitorKey = await invoke("get_window_monitor_key", { label: _label });
    } catch (e) {
        console.error("overlay: failed to get monitor key:", e);
    }
    if (!monitorKey) monitorKey = "primary"; // fallback

    try {
        config = await invoke("get_config");
        const state = await invoke("get_state");
        isMuted = state.is_muted;

        await updateIcon();
    } catch (e) {
        console.error("overlay init:", e);
    }

    if (unlistenState) unlistenState();
    unlistenState = await listen("state-update", e => {
        isMuted = e.payload.is_muted;
        updateIcon();
        // Update VU dot if needed
        const dot = document.getElementById("vu-dot");
        if (dot) {
            const myCfg = getMyConfig();
            if (myCfg?.show_vu && !isMuted) {
                const threshold = (myCfg.sensitivity ?? 5) / 100;
                dot.classList.toggle("active", e.payload.peak_level > threshold);
            } else {
                dot.classList.remove("active");
            }
        }
    });

    if (unlistenConfig) unlistenConfig();
    unlistenConfig = await listen("config-update", async e => {
        config = e.payload.config;
        // Re-query monitor key in case the mapping was populated after our
        // initial init (race during startup).
        try {
            const key = await invoke("get_window_monitor_key", { label: _label });
            if (key) monitorKey = key;
        } catch (_) {}
        updateDragRegion();
        updateIcon();
    });

    // Periodically re-assert always-on-top.
    if (topmostIntervalId !== null) clearInterval(topmostIntervalId);
    const topmostInterval = await invoke("get_overlay_topmost_interval").catch(() => 500);
    topmostIntervalId = setInterval(async () => {
        _selfWin.setAlwaysOnTop(true).catch(() => {});
    }, topmostInterval);

    setupDragDetection();
    updateDragRegion();
}
function updateDragRegion() {
    const myCfg = getMyConfig();
    if (!myCfg) return;
    document.body.style.webkitAppRegion = myCfg.locked ? 'no-drag' : 'drag';
}

function setupDragDetection() {
    // Remove old handlers if reinitializing
    if (dragMousedownHandler) document.removeEventListener("mousedown", dragMousedownHandler);
    if (dragMouseupHandler) document.removeEventListener("mouseup", dragMouseupHandler);
    if (dragMousemoveHandler) document.removeEventListener("mousemove", dragMousemoveHandler);

    dragMousedownHandler = () => {
        if (dragTimeout) clearTimeout(dragTimeout);
        isDragging = true;
        dragTimeout = setTimeout(() => {
            if (isDragging) {
                isDragging = false;
                saveCurrentPosition();
            }
        }, 500);
    };
    dragMouseupHandler = dragMousedownHandler;

    let lastMoveTime = 0;
    dragMousemoveHandler = () => {
        const now = Date.now();
        if (now - lastMoveTime >= 50) {
            lastMoveTime = now;
            dragMousedownHandler();
        }
    };

    document.addEventListener("mousedown", dragMousedownHandler);
    document.addEventListener("mouseup", dragMouseupHandler);
    document.addEventListener("mousemove", dragMousemoveHandler);
}

async function saveCurrentPosition() {
    if (!monitorKey) return;
    try {
        const position = await _selfWin.outerPosition();
        await invoke("save_overlay_position", { monitorKey, x: position.x, y: position.y });
    } catch (e) {
        console.error("Failed to save overlay position:", e);
    }
}

async function updateIcon() {
    const icon = document.getElementById("overlay-icon");
    if (!icon) return;

    const myCfg = getMyConfig();
    if (!myCfg) return;

    let isLight = false;

    if (myCfg.theme === "Light") {
        isLight = true;
    } else if (myCfg.theme === "Dark") {
        isLight = false;
    } else {
        // Auto mode: sample the background behind this specific overlay window
        try {
            isLight = await invoke("get_overlay_background_is_light", { windowLabel: _label });
        } catch (e) {
            console.error("Failed to get background theme:", e);
            isLight = window.matchMedia("(prefers-color-scheme: light)").matches;
        }
    }

    const opacity = (myCfg.opacity ?? 80) / 100;

    let src;
    if (isMuted) {
        src = isLight ? "assets/mic_muted_black.svg" : "assets/mic_muted_white.svg";
    } else {
        src = isLight ? "assets/mic_black.svg" : "assets/mic_white.svg";
    }

    icon.src = src;
    const size = myCfg.scale ?? 48;
    icon.style.width = size + "px";
    icon.style.height = size + "px";
    icon.style.opacity = opacity;

    const dot = document.getElementById("vu-dot"); 
    if (dot) {
        dot.style.display = myCfg.show_vu ? "block" : "none";
    }
}

window.addEventListener("DOMContentLoaded", init);

// Cleanup all resources on window unload to prevent accumulation
window.addEventListener("beforeunload", () => {
    if (topmostIntervalId !== null) { clearInterval(topmostIntervalId); topmostIntervalId = null; }
    if (unlistenState) { unlistenState(); unlistenState = null; }
    if (unlistenConfig) { unlistenConfig(); unlistenConfig = null; }
    if (dragMousedownHandler) document.removeEventListener("mousedown", dragMousedownHandler);
    if (dragMouseupHandler) document.removeEventListener("mouseup", dragMouseupHandler);
    if (dragMousemoveHandler) document.removeEventListener("mousemove", dragMousemoveHandler);
    if (dragTimeout) { clearTimeout(dragTimeout); dragTimeout = null; }
});

