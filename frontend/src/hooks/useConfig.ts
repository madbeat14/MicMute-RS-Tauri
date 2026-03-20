import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AppConfig } from "../types/config";
import * as cmd from "../lib/commands";

export function useConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const saveTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    cmd.getConfig().then(setConfig).catch(console.error);
    const unlisten = listen<{ config: AppConfig }>("config-update", (e) => {
      setConfig(e.payload.config);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const saveConfig = useCallback((cfg: AppConfig) => {
    cmd.updateConfig(cfg).catch(console.error);
  }, []);

  const debouncedSave = useCallback((cfg: AppConfig) => {
    if (saveTimeout.current) clearTimeout(saveTimeout.current);
    saveTimeout.current = setTimeout(() => saveConfig(cfg), 300);
  }, [saveConfig]);

  const updateConfig = useCallback((updater: (prev: AppConfig) => AppConfig) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = updater(prev);
      debouncedSave(next);
      return next;
    });
  }, [debouncedSave]);

  return { config, setConfig, updateConfig, saveConfig: debouncedSave };
}
