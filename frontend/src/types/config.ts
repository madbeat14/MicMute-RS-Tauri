export interface BeepConfig {
  freq: number;
  duration: number;
  count: number;
}

export interface AfkConfig {
  enabled: boolean;
  timeout: number;
}

export interface SoundConfig {
  file: string;
  volume: number;
}

export interface OverlayConfig {
  enabled: boolean;
  show_vu: boolean;
  opacity: number;
  x: number;
  y: number;
  position_mode: string;
  locked: boolean;
  sensitivity: number;
  device_id: string | null;
  scale: number;
  theme: string;
}

export interface OsdConfig {
  enabled: boolean;
  duration: number;
  position: string;
  size: number;
  opacity: number;
}

export interface HotkeyEntry {
  vk: number;
  name: string;
}

export interface AppConfig {
  device_id: string | null;
  sync_ids: string[];
  beep_enabled: boolean;
  audio_mode: string;
  beep_config: Record<string, BeepConfig>;
  sound_config: Record<string, SoundConfig>;
  hotkey: Record<string, HotkeyEntry>;
  hotkey_mode: string;
  afk: AfkConfig;
  persistent_overlay: OverlayConfig;
  osd: OsdConfig;
}

export interface AppStateDto {
  is_muted: boolean;
  peak_level: number;
}

export interface DeviceDto {
  id: string;
  name: string;
}
