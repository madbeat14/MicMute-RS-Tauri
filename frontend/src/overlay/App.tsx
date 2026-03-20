import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppConfig } from "../types/config";
import * as cmd from "../lib/commands";

function App() {
  const [isMuted, setIsMuted] = useState(false);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [isLight, setIsLight] = useState(false);
  const [vuActive, setVuActive] = useState(false);

  const configRef = useRef<AppConfig | null>(null);
  const isMutedRef = useRef(false);
  const isDraggingRef = useRef(false);
  const dragTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const vuPollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Keep refs in sync with state
  useEffect(() => {
    configRef.current = config;
  }, [config]);
  useEffect(() => {
    isMutedRef.current = isMuted;
  }, [isMuted]);

  // Resolve the icon theme (Auto/Dark/Light)
  const resolveTheme = useCallback(async (cfg: AppConfig): Promise<boolean> => {
    const theme = cfg.persistent_overlay.theme;
    if (theme === "Light") return true;
    if (theme === "Dark") return false;
    // Auto mode
    try {
      return await cmd.getOverlayBackgroundIsLight();
    } catch {
      return window.matchMedia("(prefers-color-scheme: light)").matches;
    }
  }, []);

  // Update theme whenever config or mute state changes
  const refreshTheme = useCallback(
    async (cfg: AppConfig) => {
      const light = await resolveTheme(cfg);
      setIsLight(light);
    },
    [resolveTheme],
  );

  // Save current window position after drag ends
  const saveCurrentPosition = useCallback(async () => {
    try {
      const win = getCurrentWindow();
      const position = await win.outerPosition();
      await cmd.saveOverlayPosition(position.x, position.y);
    } catch (e) {
      console.error("Failed to save overlay position:", e);
    }
  }, []);

  // VU meter polling
  const startVuPoll = useCallback(() => {
    if (vuPollTimerRef.current) clearInterval(vuPollTimerRef.current);

    vuPollTimerRef.current = setInterval(async () => {
      const cfg = configRef.current;
      if (!cfg?.persistent_overlay?.show_vu || isMutedRef.current) {
        setVuActive(false);
        return;
      }
      try {
        const s = await cmd.getState();
        const threshold = (cfg.persistent_overlay.sensitivity ?? 5) / 100;
        setVuActive(s.peak_level > threshold);
      } catch {
        // ignore
      }
    }, 60);
  }, []);

  // Drag detection setup
  useEffect(() => {
    const resetDragTimeout = () => {
      if (dragTimeoutRef.current) clearTimeout(dragTimeoutRef.current);
      isDraggingRef.current = true;
      dragTimeoutRef.current = setTimeout(() => {
        if (isDraggingRef.current) {
          isDraggingRef.current = false;
          saveCurrentPosition();
        }
      }, 500);
    };

    document.addEventListener("mousedown", resetDragTimeout);
    document.addEventListener("mouseup", resetDragTimeout);
    document.addEventListener("mousemove", resetDragTimeout);

    return () => {
      document.removeEventListener("mousedown", resetDragTimeout);
      document.removeEventListener("mouseup", resetDragTimeout);
      document.removeEventListener("mousemove", resetDragTimeout);
      if (dragTimeoutRef.current) clearTimeout(dragTimeoutRef.current);
    };
  }, [saveCurrentPosition]);

  // Main initialization
  useEffect(() => {
    let cancelled = false;

    async function init() {
      try {
        const [cfg, state] = await Promise.all([
          cmd.getConfig(),
          cmd.getState(),
        ]);
        if (cancelled) return;
        setConfig(cfg);
        setIsMuted(state.is_muted);
      } catch (e) {
        console.error("overlay init:", e);
      }
    }
    init();

    // Listen for state updates
    const unlistenState = listen<{ is_muted: boolean; peak_level: number }>(
      "state-update",
      (e) => {
        setIsMuted(e.payload.is_muted);
      },
    );

    // Listen for config updates
    const unlistenConfig = listen<{ config: AppConfig }>(
      "config-update",
      (e) => {
        setConfig(e.payload.config);
      },
    );

    return () => {
      cancelled = true;
      unlistenState.then((fn) => fn());
      unlistenConfig.then((fn) => fn());
    };
  }, []);

  // Periodic config refresh and always-on-top re-assertion
  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;

    async function setup() {
      const interval = await cmd
        .getOverlayTopmostInterval()
        .catch(() => 500);
      const overlayWin = getCurrentWindow();

      timer = setInterval(async () => {
        const newConfig = await cmd.getConfig().catch(() => null);
        if (newConfig) {
          setConfig(newConfig);
        }
        overlayWin.setAlwaysOnTop(true).catch(() => {});
      }, interval);
    }
    setup();

    return () => {
      if (timer) clearInterval(timer);
    };
  }, []);

  // Start VU polling on mount, restart when show_vu changes
  useEffect(() => {
    startVuPoll();
    return () => {
      if (vuPollTimerRef.current) clearInterval(vuPollTimerRef.current);
    };
  }, [startVuPoll]);

  // Refresh theme when config or mute state changes
  useEffect(() => {
    if (config) {
      refreshTheme(config);
    }
  }, [config, isMuted, refreshTheme]);

  // Update drag region on body whenever locked state changes
  useEffect(() => {
    if (!config) return;
    (document.body.style as any).webkitAppRegion = config.persistent_overlay.locked
      ? "no-drag"
      : "drag";
  }, [config?.persistent_overlay?.locked]);

  // Derive icon src
  let iconSrc = "assets/mic_white.svg";
  if (config) {
    if (isMuted) {
      iconSrc = isLight
        ? "assets/mic_muted_black.svg"
        : "assets/mic_muted_white.svg";
    } else {
      iconSrc = isLight ? "assets/mic_black.svg" : "assets/mic_white.svg";
    }
  }

  const size = config?.persistent_overlay?.scale ?? 48;
  const opacity = (config?.persistent_overlay?.opacity ?? 80) / 100;
  const showVu = config?.persistent_overlay?.show_vu ?? false;

  return (
    <>
      <img
        id="overlay-icon"
        className="overlay-icon"
        src={iconSrc}
        alt="mic status"
        style={{
          width: `${size}px`,
          height: `${size}px`,
          opacity,
        }}
      />
      <div
        id="vu-dot"
        className={`vu-dot${vuActive ? " active" : ""}`}
        style={{ display: showVu ? "block" : "none" }}
      />
    </>
  );
}

export default App;
