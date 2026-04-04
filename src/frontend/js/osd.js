// OSD (On-Screen Display) window logic
// This script handles the visual popup that appears briefly when the microphone is muted or unmuted.

// Each OSD window filters events by target label to avoid cross-window interference.
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
const _selfLabel = getCurrentWindow().label;

let unlistenOsdShow = null;

/**
 * Initializes the OSD script by setting up a listener for the "osd-show" event.
 * When the rust backend triggers this event, the OSD will be displayed.
 */
async function init() {
    if (unlistenOsdShow) unlistenOsdShow();
    unlistenOsdShow = await listen("osd-show", e => {
        if (e.payload.target && e.payload.target !== _selfLabel) return;
        showOsd(e.payload.is_muted, e.payload.duration, e.payload.opacity, e.payload.theme, e.payload.is_system_light);
    });
}

let hideTimer = null;

/**
 * Displays the OSD card with the appropriate icon and automatically hides it after a duration.
 * @param {boolean} isMuted - Whether the microphone is currently muted.
 * @param {number} duration - The duration in milliseconds to show the OSD before fading out.
 * @param {number} opacity - The opacity percentage (0-100) for the OSD card.
 */
function showOsd(isMuted, duration, opacity, theme, isSystemLight) {
    if (hideTimer) clearTimeout(hideTimer);
    const card = document.getElementById("osd-card");
    const icon = document.getElementById("osd-icon");
    if (!icon || !card) return;

    // Theme describes the icon color: "Dark" = dark/black icons, "Light" = light/white icons.
    let useDarkIcon;
    if (theme === "Dark") {
        useDarkIcon = true;
    } else if (theme === "Light") {
        useDarkIcon = false;
    } else {
        // Auto: use system theme from the backend (registry check).
        // Light system → dark icons for contrast; dark system → light icons.
        useDarkIcon = !!isSystemLight;
    }

    if (isMuted) {
        icon.src = useDarkIcon ? "assets/mic_muted_black.svg" : "assets/mic_muted_white.svg";
    } else {
        icon.src = useDarkIcon ? "assets/mic_black.svg" : "assets/mic_white.svg";
    }

    // Reset animation
    card.classList.remove("hiding");
    card.style.animation = "none";
    card.offsetHeight; // reflow
    card.style.animation = "";
    card.style.opacity = (opacity ?? 80) / 100;

    // Fade out ~300ms before end
    hideTimer = setTimeout(() => {
        card.classList.add("hiding");
        hideTimer = null;
    }, Math.max(0, duration - 300));
}

// Initialize the OSD script once the DOM is fully loaded.
window.addEventListener("DOMContentLoaded", init);

// Cleanup on window unload to prevent listener accumulation
window.addEventListener("beforeunload", () => {
    if (unlistenOsdShow) { unlistenOsdShow(); unlistenOsdShow = null; }
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
});
