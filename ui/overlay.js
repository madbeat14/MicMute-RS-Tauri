// Overlay window logic
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let config = null;
let isMuted = false;
let vuPollTimer = null;

async function init() {
    try {
        config = await invoke("get_config");
        const state = await invoke("get_state");
        isMuted = state.is_muted;
        updateIcon();
        startVuPoll();
    } catch (e) { console.error("overlay init:", e); }

    await listen("state-update", e => {
        isMuted = e.payload.is_muted;
        updateIcon();
    });

    // Refresh config periodically (catches settings changes)
    setInterval(async () => {
        config = await invoke("get_config").catch(() => config);
        updateIcon();
    }, 2000);
}

function updateIcon() {
    const icon = document.getElementById("overlay-icon");
    if (!icon || !config) return;

    const isLight = config.persistent_overlay.theme === "Light" ||
        (config.persistent_overlay.theme === "Auto" && window.matchMedia("(prefers-color-scheme: light)").matches);
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
    }, 60);
}

window.addEventListener("DOMContentLoaded", init);
