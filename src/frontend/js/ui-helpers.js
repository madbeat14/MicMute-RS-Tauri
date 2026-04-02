// UI DOM Helpers

/**
 * Synchronizes an HTML range slider value with its adjacent text label.
 */
function setSlider(id, value, labelId) {
    const el = document.getElementById(id);
    const lbl = document.getElementById(labelId);
    if (el) el.value = value;
    if (lbl) lbl.textContent = value;
}

/**
 * Sets the active option in an HTML `<select>` element.
 */
function setSelect(id, value) {
    const el = document.getElementById(id);
    if (!el) return;
    el.value = value;
}

/**
 * Binds an HTML range slider to automatically update its text label and invoke 
 * a callback whenever the user scrubs the slider thumb.
 */
function bindSlider(sliderId, labelId, onValue) {
    const el = document.getElementById(sliderId);
    const lbl = document.getElementById(labelId);
    if (!el) return;
    el.addEventListener("input", () => {
        const v = parseInt(el.value);
        if (lbl) lbl.textContent = v;
        onValue(v);
        if (typeof debouncedSave === 'function') debouncedSave();
    });
}

/**
 * Visually enables or disables a block of options depending on a master checkbox state.
 */
function updateSubOptions(checkId, optionsId) {
    const chk = document.getElementById(checkId);
    const opts = document.getElementById(optionsId);
    if (!chk || !opts) return;
    opts.style.opacity = chk.checked ? "1" : "0.4";
    opts.style.pointerEvents = chk.checked ? "auto" : "none";
}

/**
 * Switches the active tab in the settings menu.
 */
function switchTab(tabId) {
    document.querySelectorAll('.tab-pane').forEach(pane => {
        pane.classList.remove('active');
    });
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.remove('active');
    });
    document.getElementById(tabId).classList.add('active');
    const btnId = 'btn-' + tabId;
    const btn = document.getElementById(btnId);
    if (btn) btn.classList.add('active');
}

/**
 * Measures all tab panes to find the tallest, then resizes the window once
 */
async function autoFitWindow() {
    await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
    const { getCurrentWindow } = window.__TAURI__.window;
    const win = getCurrentWindow();
    const tabsNav = document.querySelector('.tabs-nav');
    let tabsWidth = 0;
    tabsNav.querySelectorAll('.tab-btn').forEach(btn => {
        tabsWidth += btn.offsetWidth;
    });
    const tabCount = tabsNav.querySelectorAll('.tab-btn').length;
    tabsWidth += (tabCount - 1) * 4 + 28;

    const panes = document.querySelectorAll('.tab-pane');
    const content = document.querySelector('.settings-content');
    const prevFlex = content.style.flex;
    const prevOverflow = content.style.overflow;
    content.style.flex = '0 0 auto';
    content.style.overflow = 'visible';

    let maxPaneH = 0;
    try {
        panes.forEach(pane => {
            const wasActive = pane.classList.contains('active');
            if (!wasActive) pane.classList.add('active');
            const h = pane.scrollHeight;
            if (h > maxPaneH) maxPaneH = h;
            if (!wasActive) pane.classList.remove('active');
        });
    } finally {
        content.style.flex = prevFlex;
        content.style.overflow = prevOverflow;
    }

    const header = document.querySelector('.app-header');
    const footer = document.querySelector('.app-footer');
    const totalHeight = header.offsetHeight + tabsNav.offsetHeight + maxPaneH + 24 + footer.offsetHeight;

    const desiredW = Math.max(tabsWidth, 480);
    const screenH = window.screen.availHeight;
    const finalH = Math.min(totalHeight, Math.floor(screenH * 0.9));
    const { LogicalSize } = window.__TAURI__.window;
    await win.setSize(new LogicalSize(desiredW, finalH));
}

let debugTimer = null;
/**
 * Prints a temporary message to the debug status label at the bottom of the window.
 */
function showDebug(msg) {
    const el = document.getElementById("debug-msg");
    if (el) el.textContent = msg;
    if (debugTimer) clearTimeout(debugTimer);
    debugTimer = setTimeout(() => { if (el) el.textContent = ""; }, 3000);
}

/**
 * Updates the width of the Volume Unit (VU) meter bar.
 */
function updateVU(peak) {
    const bar = document.getElementById("vu-bar");
    if (!bar) return;
    const threshold = (config?.persistent_overlay?.sensitivity ?? 5) / 100;
    if (peak < threshold) {
        bar.style.width = "0%";
        return;
    }
    const above = (peak - threshold) / (1 - threshold);
    const scaled = Math.pow(Math.min(1, above * 10), 0.5) * 100;
    bar.style.width = Math.min(100, scaled) + "%";
}

/**
 * Updates the text and style of the mute status badge and toggle button.
 */
function updateMuteUI(muted) {
    const badge = document.getElementById("mute-status");
    const btn = document.getElementById("btn-toggle-mute");
    badge.textContent = muted ? "Muted" : "Active";
    badge.className = "status-badge " + (muted ? "muted" : "active");
    btn.textContent = muted ? "Mute" : "Mic";
    btn.setAttribute("aria-label", muted ? "Unmute microphone" : "Mute microphone");
    btn.setAttribute("aria-pressed", String(muted));
}
