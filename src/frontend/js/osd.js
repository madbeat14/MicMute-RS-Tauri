// OSD (On-Screen Display) window logic
// This script handles the visual popup that appears briefly when the microphone is muted or unmuted.

// Import the Tauri event listener function
const { listen } = window.__TAURI__.event;

/**
 * Initializes the OSD script by setting up a listener for the "osd-show" event.
 * When the rust backend triggers this event, the OSD will be displayed.
 */
async function init() {
    await listen("osd-show", e => {
        showOsd(e.payload.is_muted, e.payload.duration, e.payload.opacity);
    });
}

let hideTimer = null;

/**
 * Displays the OSD card with the appropriate icon and automatically hides it after a duration.
 * @param {boolean} isMuted - Whether the microphone is currently muted.
 * @param {number} duration - The duration in milliseconds to show the OSD before fading out.
 * @param {number} opacity - The opacity percentage (0-100) for the OSD card.
 */
function showOsd(isMuted, duration, opacity) {
    if (hideTimer) clearTimeout(hideTimer);
    const card = document.getElementById("osd-card");
    const icon = document.getElementById("osd-icon");
    if (!icon || !card) return;

    const isLight = window.matchMedia("(prefers-color-scheme: light)").matches;
    if (isMuted) {
        icon.src = isLight ? "assets/mic_muted_black.svg" : "assets/mic_muted_white.svg";
    } else {
        icon.src = isLight ? "assets/mic_black.svg" : "assets/mic_white.svg";
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
