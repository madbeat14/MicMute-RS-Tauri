// Audio UI logic

/**
 * Toggles visibility between Beep and Custom sound controls.
 */
function updateAudioModeUI(mode) {
    document.getElementById("audio-beep-controls").style.display = (mode === "beep") ? "block" : "none";
    document.getElementById("audio-custom-controls").style.display = (mode === "custom") ? "block" : "none";
}

/**
 * Triggers a file picker via Tauri's dialog plugin.
 */
async function pickAudioFile(key) {
    try {
        const path = await window.__TAURI__.core.invoke("pick_audio_file");
        if (path) {
            if (!config.sound_config) config.sound_config = {};
            if (!config.sound_config[key]) config.sound_config[key] = { file: "", volume: 50 };
            config.sound_config[key].file = path;
            document.getElementById(`path-${key}`).value = path;
            if (typeof debouncedSave === 'function') debouncedSave();
        }
    } catch (e) {
        if (typeof showDebug === 'function') showDebug("File picking failed: " + e);
    }
}

/**
 * Plays a preview of the sound using current UI parameters without saving first.
 */
async function previewAudio(mode, key) {
    try {
        const payload = JSON.stringify(config);
        await window.__TAURI__.core.invoke("preview_audio_feedback", { mode, key, payload });
    } catch (e) {
        if (typeof showDebug === 'function') showDebug("Preview failed: " + e);
    }
}
