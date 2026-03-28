// Overlay window logic
// This script maintains the state and updates the UI for the persistent floating mic status overlay.

// Import Tauri commands and event APIs
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// State variables
let config = null;      // Holds the application configuration
let isMuted = false;    // Current microphone mute status
let vuPollTimer = null; // Timer reference for polling the Volume Unit (VU) meter
let isDragging = false; // Track if the window is being dragged
let dragTimeout = null; // Timeout to detect when dragging ends

let unlistenState = null;
let unlistenConfig = null;

/**
 * Initializes the overlay by fetching the initial configuration and state,
 * subscribing to state updates, and starting the VU meter polling if enabled.
 * Also sets up drag detection to save position when user finishes dragging.
 */
async function init() {
    try {
        config = await invoke("get_config");
        const state = await invoke("get_state");
        isMuted = state.is_muted;

        await updateIcon();
        startVuPoll();
    } catch (e) {
        console.error("overlay init:", e);
    }

    if (unlistenState) unlistenState();
    unlistenState = await listen("state-update", e => {
        isMuted = e.payload.is_muted;
        updateIcon();
    });

    // Listen for config updates from the main window
    if (unlistenConfig) unlistenConfig();
    unlistenConfig = await listen("config-update", e => {
        config = e.payload.config;
        updateDragRegion();
        updateIcon();
    });

    // Refresh config and re-assert always-on-top periodically.
    // Interval matches the backend's OVERLAY_TOPMOST_INTERVAL_MS constant.
    const { getCurrentWindow } = window.__TAURI__.window;
    const overlayWin = getCurrentWindow();
    const topmostInterval = await invoke("get_overlay_topmost_interval").catch(() => 500);
    setInterval(async () => {
        const newConfig = await invoke("get_config").catch(() => null);
        if (newConfig) {
            config = newConfig;
            updateDragRegion();
            updateIcon();
        }
        overlayWin.setAlwaysOnTop(true).catch(() => {});
    }, topmostInterval);

    // Setup drag detection
    setupDragDetection();
    
    // Initial drag region setup
    updateDragRegion();
}

/**
 * Updates the -webkit-app-region CSS property based on the locked state.
 * When locked: no-drag (can't move the window)
 * When unlocked: drag (can move the window by dragging)
 */
function updateDragRegion() {
    if (!config) return;
    const body = document.body;
    if (config.persistent_overlay.locked) {
        body.style.webkitAppRegion = 'no-drag';
    } else {
        body.style.webkitAppRegion = 'drag';
    }
}

/**
 * Sets up event listeners to detect when the user finishes dragging the overlay window.
 * When dragging ends, the current position is saved to the config.
 */
function setupDragDetection() {
    // The overlay has -webkit-app-region: drag in CSS, so the OS handles dragging.
    // We detect drag end by monitoring mouse events and a timeout.
    const resetDragTimeout = () => {
        if (dragTimeout) clearTimeout(dragTimeout);
        isDragging = true;
        dragTimeout = setTimeout(() => {
            if (isDragging) {
                isDragging = false;
                // Drag has ended, save the current position
                saveCurrentPosition();
            }
        }, 500); // Wait 500ms after last mouse event to consider drag ended
    };

    // Throttle mousemove to avoid excessive timer resets
    let lastMoveTime = 0;
    const throttledMove = () => {
        const now = Date.now();
        if (now - lastMoveTime >= 50) {
            lastMoveTime = now;
            resetDragTimeout();
        }
    };

    document.addEventListener("mousedown", resetDragTimeout);
    document.addEventListener("mouseup", resetDragTimeout);
    document.addEventListener("mousemove", throttledMove);
}

/**
 * Saves the current overlay window position to the backend config.
 * This is called when the user finishes dragging the overlay.
 */
async function saveCurrentPosition() {
    try {
        // Get the current window position using the Tauri window API
        const { getCurrentWindow } = window.__TAURI__.window;
        const win = getCurrentWindow();
        const position = await win.outerPosition();
        
        // Save to config
        await invoke("save_overlay_position", { x: position.x, y: position.y });
        // Position saved successfully
    } catch (e) {
        console.error("Failed to save overlay position:", e);
    }
}

/**
 * Updates the overlay icon appearance, size, and opacity based on the current configuration,
 * background color (for auto theme), and mute status. Also handles the visibility of the VU activity dot.
 */
async function updateIcon() {
    const icon = document.getElementById("overlay-icon");
    if (!icon || !config) return;

    let isLight = false;
    
    if (config.persistent_overlay.theme === "Light") {
        isLight = true;
    } else if (config.persistent_overlay.theme === "Dark") {
        isLight = false;
    } else {
        // Auto mode: detect actual background color
        try {
            isLight = await invoke("get_overlay_background_is_light");
        } catch (e) {
            console.error("Failed to get background theme:", e);
            // Fallback to system theme detection
            isLight = window.matchMedia("(prefers-color-scheme: light)").matches;
        }
    }
    
    const opacity = (config.persistent_overlay.opacity ?? 80) / 100;

    let src;
    if (isMuted) {
        src = isLight ? "assets/mic_muted_black.svg" : "assets/mic_muted_white.svg";
    } else {
        src = isLight ? "assets/mic_black.svg" : "assets/mic_white.svg";
    }

    icon.src = src;
    const size = config.persistent_overlay.scale ?? 48;
    icon.style.width = size + "px";
    icon.style.height = size + "px";
    icon.style.opacity = opacity;

    // Show/hide VU dot
    const dot = document.getElementById("vu-dot");
    if (dot) {
        dot.style.display = config.persistent_overlay.show_vu ? "block" : "none";
        // Re-check polling if not already running
        if (config.persistent_overlay.show_vu && !vuPollTimer) {
            startVuPoll();
        }
    }
}

/**
 * Starts a polling interval that periodically queries the rust backend for the current 
 * microphone peak volume level. It will toggle the 'active' class on the VU dot 
 * if the peak exceeds the user-configured sensitivity threshold.
 */
function startVuPoll() {
    if (vuPollTimer) clearInterval(vuPollTimer);

    vuPollTimer = setInterval(async () => {
        const dot = document.getElementById("vu-dot");
        if (!dot) return;

        if (!config?.persistent_overlay?.show_vu || isMuted) {
            dot.classList.remove("active");
            return;
        }

        try {
            const s = await invoke("get_state");
            // Sensitivity 1-100 logic: lower value = more sensitive? 
            // Actually let's make it a direct threshold where 1 is hyper-sensitive and 100 is deaf.
            const threshold = (config.persistent_overlay.sensitivity ?? 5) / 100;
            dot.classList.toggle("active", s.peak_level > threshold);
        } catch (_) { }
    }, 100);
}

// Initialize the overlay script once the DOM is fully loaded.
window.addEventListener("DOMContentLoaded", init);
