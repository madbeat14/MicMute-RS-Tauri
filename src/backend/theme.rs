//! Theme detection: system-wide light/dark and per-pixel background brightness.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel,
    HDC, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

/// RAII guard for desktop DC released with `ReleaseDC`.
struct DesktopDcGuard {
    hdc: HDC,
}

impl Drop for DesktopDcGuard {
    fn drop(&mut self) {
        if !self.hdc.is_invalid() {
            unsafe {
                let _ = ReleaseDC(None, self.hdc);
            }
        }
    }
}

/// RAII guard for memory DC deleted with `DeleteDC`.
struct MemDcGuard(HDC);

impl Drop for MemDcGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }
}

/// RAII guard for GDI objects deleted with `DeleteObject`.
struct GdiObjectGuard(HGDIOBJ);

impl Drop for GdiObjectGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.0);
            }
        }
    }
}

/// Check if the Windows system theme is set to light mode via the registry.
pub fn is_system_light_theme() -> bool {
    let subkey = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect::<Vec<u16>>();

    let val_name = "SystemUsesLightTheme\0"
        .encode_utf16()
        .collect::<Vec<u16>>();

    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_ok()
        {
            let mut data: u32 = 0;
            let mut data_size = std::mem::size_of::<u32>() as u32;

            let res = RegQueryValueExW(
                hkey,
                windows::core::PCWSTR(val_name.as_ptr()),
                None,
                None,
                Some(&mut data as *mut _ as *mut u8),
                Some(&mut data_size),
            );

            let _ = RegCloseKey(hkey);

            if res.is_ok() {
                return data == 1;
            }
        }
    }
    false
}

/// Per-window hysteresis state for background brightness detection.
/// Prevents rapid icon theme toggling when the background brightness is near
/// the threshold boundary.
static PER_WINDOW_LIGHT_STATE: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<isize, bool>>,
> = std::sync::OnceLock::new();

fn get_per_window_state() -> &'static parking_lot::Mutex<std::collections::HashMap<isize, bool>> {
    PER_WINDOW_LIGHT_STATE.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()))
}

/// Captures the screen area behind the given window and determines if it's light or dark.
/// Returns true if the background is light, false if dark.
/// Used for auto theme detection on the overlay icon.
///
/// Uses per-window hysteresis (two thresholds) to prevent rapid toggling when the
/// background brightness is near the boundary. Each overlay window on different
/// monitors tracks its own state independently. White icons are the default; dark
/// icons only appear on very bright backgrounds.
pub fn is_background_light(hwnd: HWND) -> bool {
    use crate::constants::{OVERLAY_BRIGHT_THRESHOLD, OVERLAY_DIM_THRESHOLD};

    let avg_brightness = match sample_background_brightness(hwnd) {
        Some(b) => b,
        None => return is_system_light_theme(),
    };

    let hwnd_key = hwnd.0 as isize;
    let mut state_map = get_per_window_state().lock();
    let was_light = *state_map.get(&hwnd_key).unwrap_or(&false);

    let is_light = if was_light {
        // Currently dark icons — stay that way unless brightness drops enough
        avg_brightness > OVERLAY_DIM_THRESHOLD
    } else {
        // Currently white icons (default) — only switch when really bright
        avg_brightness > OVERLAY_BRIGHT_THRESHOLD
    };

    state_map.insert(hwnd_key, is_light);
    is_light
}

/// Samples the screen behind `hwnd` and returns average perceived brightness (0–255).
/// Returns `None` if the capture fails.
pub fn sample_background_brightness(hwnd: HWND) -> Option<u64> {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }

        let x = rect.left;
        let y = rect.top;
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);

        if width == 0 || height == 0 {
            return None;
        }

        let desktop_dc_raw = GetDC(None);
        if desktop_dc_raw.is_invalid() {
            return None;
        }
        let _desktop_guard = DesktopDcGuard {
            hdc: desktop_dc_raw,
        };

        let mem_dc_raw = CreateCompatibleDC(desktop_dc_raw);
        if mem_dc_raw.is_invalid() {
            return None;
        }
        let _mem_dc_guard = MemDcGuard(mem_dc_raw);

        let bitmap_raw = CreateCompatibleBitmap(desktop_dc_raw, width, height);
        if bitmap_raw.is_invalid() {
            return None;
        }
        let _bitmap_guard = GdiObjectGuard(HGDIOBJ(bitmap_raw.0));

        let old_bitmap = SelectObject(mem_dc_raw, bitmap_raw);

        let blt_ok = BitBlt(mem_dc_raw, 0, 0, width, height, desktop_dc_raw, x, y, SRCCOPY);
        if blt_ok.is_err() {
            let _ = SelectObject(mem_dc_raw, old_bitmap);
            return None;
        }

        let mut total_brightness: u64 = 0;
        let mut sample_count: u64 = 0;

        let step_x = (width / 4).max(1);
        let step_y = (height / 4).max(1);

        for sy in (0..height).step_by(step_y as usize) {
            for sx in (0..width).step_by(step_x as usize) {
                let pixel = GetPixel(mem_dc_raw, sx, sy);
                // COLORREF layout is 0x00BBGGRR (not RGB)
                let pixel_value = pixel.0;
                let r = (pixel_value & 0xFF) as u64;
                let g = ((pixel_value >> 8) & 0xFF) as u64;
                let b = ((pixel_value >> 16) & 0xFF) as u64;
                let brightness = (r * 299 + g * 587 + b * 114) / 1000;
                total_brightness += brightness;
                sample_count += 1;
            }
        }

        let _ = SelectObject(mem_dc_raw, old_bitmap);

        if sample_count == 0 {
            return None;
        }

        Some(total_brightness / sample_count)
    }
}
