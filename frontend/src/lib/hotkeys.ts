export const COMMON_KEYS: [number, string][] = [
  [0xB3, "Media Play/Pause"], [0x70, "F1"], [0x71, "F2"],
  [0x72, "F3"], [0x73, "F4"], [0x74, "F5"], [0x75, "F6"],
  [0x76, "F7"], [0x77, "F8"], [0x78, "F9"], [0x79, "F10"],
  [0x7A, "F11"], [0x7B, "F12"], [0x20, "Space"], [0x0D, "Enter"],
  [0xAD, "Volume Mute"], [0xAE, "Volume Down"], [0xAF, "Volume Up"],
];

export const JS_KEY_TO_VK: Record<string, number> = {
  'MediaPlayPause': 0xB3, 'MediaTrackNext': 0xB0, 'MediaTrackPrevious': 0xB1,
  'MediaStop': 0xB2, 'AudioVolumeMute': 0xAD, 'AudioVolumeDown': 0xAE,
  'AudioVolumeUp': 0xAF, 'F1': 0x70, 'F2': 0x71, 'F3': 0x72, 'F4': 0x73,
  'F5': 0x74, 'F6': 0x75, 'F7': 0x76, 'F8': 0x77, 'F9': 0x78, 'F10': 0x79,
  'F11': 0x7A, 'F12': 0x7B, 'Space': 0x20, 'Enter': 0x0D, 'Backspace': 0x08,
  'Tab': 0x09, 'Escape': 0x1B, 'CapsLock': 0x14, 'Pause': 0x13,
};

const VK_NAMES: Record<number, string> = {
  0x08: "Backspace", 0x09: "Tab", 0x0D: "Enter", 0x10: "Shift",
  0x11: "Ctrl", 0x12: "Alt", 0x13: "Pause", 0x14: "Caps Lock",
  0x1B: "Escape", 0x20: "Space", 0x21: "Page Up", 0x22: "Page Down",
  0x23: "End", 0x24: "Home", 0x25: "Left", 0x26: "Up",
  0x27: "Right", 0x28: "Down", 0x2C: "Print Screen", 0x2D: "Insert",
  0x2E: "Delete", 0x5B: "Left Win", 0x5C: "Right Win", 0x5D: "Menu",
  0x6A: "Numpad *", 0x6B: "Numpad +", 0x6D: "Numpad -",
  0x6E: "Numpad .", 0x6F: "Numpad /", 0x90: "Num Lock",
  0x91: "Scroll Lock", 0xA0: "Left Shift", 0xA1: "Right Shift",
  0xA2: "Left Ctrl", 0xA3: "Right Ctrl", 0xA4: "Left Alt",
  0xA5: "Right Alt", 0xBA: ";", 0xBB: "=", 0xBC: ",",
  0xBD: "-", 0xBE: ".", 0xBF: "/", 0xC0: "`",
  0xDB: "[", 0xDC: "\\", 0xDD: "]", 0xDE: "'",
};

export function vkToName(vk: number): string {
  const common = COMMON_KEYS.find(([v]) => v === vk);
  if (common) return common[1];
  if (vk >= 0x41 && vk <= 0x5A) return String.fromCharCode(vk);
  if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30);
  if (vk >= 0x60 && vk <= 0x69) return `Numpad ${vk - 0x60}`;
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x70 + 1}`;
  return VK_NAMES[vk] ?? `Key 0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

export function jsEventToVK(e: KeyboardEvent): number {
  let vk = JS_KEY_TO_VK[e.key] || JS_KEY_TO_VK[e.code] || 0;
  if (!vk && e.keyCode >= 0x08) vk = e.keyCode;
  return vk;
}
