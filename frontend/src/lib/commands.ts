import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, AppStateDto, DeviceDto } from "../types/config";

export const getState = () => invoke<AppStateDto>("get_state");
export const toggleMute = () => invoke<AppStateDto>("toggle_mute");
export const setMute = (muted: boolean) => invoke<AppStateDto>("set_mute", { muted });
export const getConfig = () => invoke<AppConfig>("get_config");
export const updateConfig = (config: AppConfig) =>
  invoke("update_config", { payload: JSON.stringify(config) });
export const getCachedDevices = () => invoke<DeviceDto[]>("get_cached_devices");
export const getDevices = () => invoke<DeviceDto[]>("get_devices");
export const setDevice = (deviceId: string | null) => invoke("set_device", { deviceId });
export const startRecordingHotkey = () => invoke("start_recording_hotkey");
export const stopRecordingHotkey = () => invoke("stop_recording_hotkey");
export const getRecordedHotkey = () => invoke<number | null>("get_recorded_hotkey");
export const setRunOnStartup = (enable: boolean) =>
  invoke("set_run_on_startup_cmd", { enable });
export const getRunOnStartup = () => invoke<boolean>("get_run_on_startup_cmd");
export const pickAudioFile = () => invoke<string | null>("pick_audio_file");
export const previewAudioFeedback = (mode: string, key: string, config: AppConfig) =>
  invoke("preview_audio_feedback", { mode, key, payload: JSON.stringify(config) });
export const openUrl = (url: string) => invoke("open_url", { url });
export const getOverlayBackgroundIsLight = () =>
  invoke<boolean>("get_overlay_background_is_light");
export const getOverlayTopmostInterval = () =>
  invoke<number>("get_overlay_topmost_interval");
export const saveOverlayPosition = (x: number, y: number) =>
  invoke("save_overlay_position", { x, y });
