//! Theme detection: system-wide light/dark and per-pixel background brightness.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel,
    ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

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

            let _ = windows::Win32::System::Registry::RegCloseKey(hkey);

            if res.is_ok() {
                return data == 1;
            }
        }
    }
    false
}

/// Captures the screen area behind the given window and determines if it's light or dark.
/// Returns true if the background is light, false if dark.
/// Used for auto theme detection on the overlay icon.
///
/// Uses hysteresis (two thresholds) to prevent rapid toggling when the background
/// brightness is near the boundary. White icons are the default; dark icons only
/// appear on very bright backgrounds.
pub fn is_background_light(hwnd: HWND) -> bool {
    use crate::constants::{OVERLAY_BRIGHT_THRESHOLD, OVERLAY_DIM_THRESHOLD};
    use std::sync::atomic::{AtomicBool, Ordering};

    static LAST_IS_LIGHT: AtomicBool = AtomicBool::new(false);

    let avg_brightness = match sample_background_brightness(hwnd) {
        Some(b) => b,
        None => return is_system_light_theme(),
    };

    let was_light = LAST_IS_LIGHT.load(Ordering::Relaxed);
    let is_light = if was_light {
        // Currently dark icons — stay that way unless brightness drops enough
        avg_brightness > OVERLAY_DIM_THRESHOLD
    } else {
        // Currently white icons (default) — only switch when really bright
        avg_brightness > OVERLAY_BRIGHT_THRESHOLD
    };

    LAST_IS_LIGHT.store(is_light, Ordering::Relaxed);
    is_light
}

/// Samples the screen behind `hwnd` and returns average perceived brightness (0–255).
/// Returns `None` if the capture fails.
fn sample_background_brightness(hwnd: HWND) -> Option<u64> {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }

        let x = rect.left;
        let y = rect.top;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let desktop_dc = GetDC(None);
        if desktop_dc.is_invalid() {
            return None;
        }

        let mem_dc = CreateCompatibleDC(desktop_dc);
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, desktop_dc);
            return None;
        }

        let bitmap = CreateCompatibleBitmap(desktop_dc, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, desktop_dc);
            return None;
        }

        let old_bitmap = SelectObject(mem_dc, bitmap);

        let _ = BitBlt(mem_dc, 0, 0, width, height, desktop_dc, x, y, SRCCOPY);

        let mut total_brightness: u64 = 0;
        let mut sample_count: u64 = 0;

        let step_x = (width / 4).max(1);
        let step_y = (height / 4).max(1);

        for sy in (0..height).step_by(step_y as usize) {
            for sx in (0..width).step_by(step_x as usize) {
                let pixel = GetPixel(mem_dc, sx, sy);
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

        let _ = SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, desktop_dc);

        if sample_count == 0 {
            return None;
        }

        Some(total_brightness / sample_count)
    }
}
