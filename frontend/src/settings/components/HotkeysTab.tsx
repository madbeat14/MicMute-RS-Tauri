import { useState, useRef, useEffect, useCallback } from "react";
import type { AppConfig } from "../../types/config";
import { Card } from "./ui/Card";
import { COMMON_KEYS, vkToName, jsEventToVK } from "../../lib/hotkeys";
import * as cmd from "../../lib/commands";

interface Props {
  config: AppConfig;
  updateConfig: (updater: (prev: AppConfig) => AppConfig) => void;
}

export function HotkeysTab({ config, updateConfig }: Props) {
  const [recordingKey, setRecordingKey] = useState<string | null>(null);
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const finishRecording = useCallback((key: string, vk: number) => {
    if (pollTimer.current) { clearInterval(pollTimer.current); pollTimer.current = null; }
    if (timeoutTimer.current) { clearTimeout(timeoutTimer.current); timeoutTimer.current = null; }
    setRecordingKey(null);
    cmd.stopRecordingHotkey().catch(() => {});
    updateConfig((prev) => ({
      ...prev,
      hotkey: { ...prev.hotkey, [key]: { vk, name: vkToName(vk) } },
    }));
  }, [updateConfig]);

  const cancelRecording = useCallback(() => {
    if (pollTimer.current) { clearInterval(pollTimer.current); pollTimer.current = null; }
    if (timeoutTimer.current) { clearTimeout(timeoutTimer.current); timeoutTimer.current = null; }
    setRecordingKey(null);
    cmd.stopRecordingHotkey().catch(() => {});
  }, []);

  const startRecording = useCallback(async (key: string) => {
    setRecordingKey(key);
    await cmd.startRecordingHotkey();
    pollTimer.current = setInterval(async () => {
      const vk = await cmd.getRecordedHotkey();
      if (vk !== null && vk !== undefined) {
        finishRecording(key, vk);
      }
    }, 100);
    timeoutTimer.current = setTimeout(() => {
      if (recordingKey === key) cancelRecording();
    }, 10000);
  }, [finishRecording, cancelRecording, recordingKey]);

  // Keydown handler for recording when WebView has focus
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!recordingKey) return;
      const vk = jsEventToVK(e);
      if (vk) {
        e.preventDefault();
        e.stopPropagation();
        finishRecording(recordingKey, vk);
      }
    };
    document.addEventListener("keydown", handler, true);
    return () => document.removeEventListener("keydown", handler, true);
  }, [recordingKey, finishRecording]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (pollTimer.current) clearInterval(pollTimer.current);
      if (timeoutTimer.current) clearTimeout(timeoutTimer.current);
    };
  }, []);

  const mode = config.hotkey_mode;
  const keys = mode === "toggle" ? ["toggle"] : ["mute", "unmute"];

  const clearKey = (key: string) => {
    updateConfig((prev) => ({
      ...prev,
      hotkey: { ...prev.hotkey, [key]: { vk: 0, name: "None" } },
    }));
  };

  const selectKey = (key: string, vk: number) => {
    updateConfig((prev) => ({
      ...prev,
      hotkey: { ...prev.hotkey, [key]: { vk, name: vkToName(vk) } },
    }));
  };

  return (
    <Card title="Hotkeys" icon="⌨️">
      <div className="radio-group">
        <label>
          <input
            type="radio"
            name="hk-mode"
            checked={mode === "toggle"}
            onChange={() => updateConfig((p) => ({ ...p, hotkey_mode: "toggle" }))}
          /> Single Toggle Key
        </label>
        <label>
          <input
            type="radio"
            name="hk-mode"
            checked={mode === "separate"}
            onChange={() => updateConfig((p) => ({ ...p, hotkey_mode: "separate" }))}
          /> Separate Mute/Unmute Keys
        </label>
      </div>
      <div className="hotkey-rows mt-12">
        {keys.map((key) => {
          const hk = config.hotkey[key] || { vk: 0, name: "None" };
          const currentVk = hk.vk ?? 0;
          const isRecording = recordingKey === key;

          // Build options: COMMON_KEYS + current custom key if not in list
          let options = COMMON_KEYS;
          if (currentVk && !COMMON_KEYS.some(([vk]) => vk === currentVk)) {
            options = [[currentVk, hk.name || vkToName(currentVk)] as [number, string], ...COMMON_KEYS];
          }

          return (
            <div key={key} className="hotkey-row">
              <label>{key.charAt(0).toUpperCase() + key.slice(1)}:</label>
              <select
                className="select-input"
                value={currentVk}
                onChange={(e) => selectKey(key, Number(e.target.value))}
              >
                {options.map(([vk, name]) => (
                  <option key={vk} value={vk}>{name}</option>
                ))}
              </select>
              <button
                className={`btn-sm ${isRecording ? "recording" : ""}`}
                onClick={() => isRecording ? cancelRecording() : startRecording(key)}
              >
                {isRecording ? "…" : "Record"}
              </button>
              <button className="btn-sm" onClick={() => clearKey(key)}>Clear</button>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
