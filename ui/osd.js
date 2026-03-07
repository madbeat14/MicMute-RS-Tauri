// OSD window logic
const { listen } = window.__TAURI__.event;

async function init() {
    await listen("osd-show", e => {
        showOsd(e.payload.is_muted, e.payload.duration);
    });
}

function showOsd(isMuted, duration) {
    const card = document.getElementById("osd-card");
    const icon = document.getElementById("osd-icon");
    if (!icon || !card) return;

    const isLight = window.matchMedia("(prefers-color-scheme: light)").matches;
    if (isMuted) {
        icon.src = isLight ? "../assets/mic_muted_black.svg" : "../assets/mic_muted_white.svg";
    } else {
        icon.src = isLight ? "../assets/mic_black.svg" : "../assets/mic_white.svg";
    }

    // Reset animation
    card.classList.remove("hiding");
    card.style.animation = "none";
    card.offsetHeight; // reflow
    card.style.animation = "";
    card.style.opacity = "1";

    // Fade out ~300ms before end
    setTimeout(() => {
        card.classList.add("hiding");
    }, Math.max(0, duration - 300));
}

window.addEventListener("DOMContentLoaded", init);
