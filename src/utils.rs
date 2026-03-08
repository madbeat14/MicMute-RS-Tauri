use windows::Win32::System::Registry::{RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER};
use windows::Win32::System::Registry::KEY_READ;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GetDC, ReleaseDC, CreateCompatibleBitmap, SelectObject, DeleteObject, GetPixel, BitBlt, SRCCOPY};

pub fn is_system_light_theme() -> bool {
    let subkey = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect::<Vec<u16>>();
        
    let val_name = "SystemUsesLightTheme\0"
        .encode_utf16()
        .collect::<Vec<u16>>();

    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, windows::core::PCWSTR(subkey.as_ptr()), 0, KEY_READ, &mut hkey).is_ok() {
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

pub fn get_idle_duration() -> f32 {
    unsafe {
        let mut last_input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        
        let ok_val: bool = GetLastInputInfo(&mut last_input).into();
        if ok_val {
            let ticks = GetTickCount();
            let millis = ticks.saturating_sub(last_input.dwTime);
            return (millis as f32) / 1000.0;
        }
    }
    0.0
}

pub fn vk_to_string(vk: u32) -> String {
    match vk {
        0 => "None".to_string(),
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x10 => "Shift".to_string(),
        0x11 => "Ctrl".to_string(),
        0x12 => "Alt".to_string(),
        0x13 => "Pause".to_string(),
        0x14 => "Caps Lock".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x30..=0x39 => format!("{}", (vk - 0x30) as u8),
        0x41..=0x5A => format!("{}", ((vk - 0x41) as u8 + b'A') as char),
        0x60..=0x69 => format!("Numpad {}", (vk - 0x60) as u8),
        0x70..=0x87 => format!("F{}", (vk - 0x70) + 1),
        0xA0 => "LShift".to_string(),
        0xA1 => "RShift".to_string(),
        0xA2 => "LCtrl".to_string(),
        0xA3 => "RCtrl".to_string(),
        0xA4 => "LAlt".to_string(),
        0xA5 => "RAlt".to_string(),
        0xAF => "Volume Up".to_string(),
        0xAE => "Volume Down".to_string(),
        0xAD => "Volume Mute".to_string(),
        0xB0 => "Media Next".to_string(),
        0xB1 => "Media Prev".to_string(),
        0xB2 => "Media Stop".to_string(),
        0xB3 => "Media Play/Pause".to_string(),
        _ => format!("VK_0x{:02X}", vk),
    }
}

/// Captures the screen area behind the given window and determines if it's light or dark.
/// Returns true if the background is light, false if dark.
/// This is used for auto theme detection on the overlay icon.
pub fn is_background_light(hwnd: HWND) -> bool {
    unsafe {
        // Get the window position
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return is_system_light_theme(); // Fallback to system theme
        }

        let x = rect.left;
        let y = rect.top;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // Get the desktop DC
        let desktop_dc = GetDC(None);
        if desktop_dc.is_invalid() {
            return is_system_light_theme();
        }

        // Create a compatible DC and bitmap for capturing
        let mem_dc = CreateCompatibleDC(desktop_dc);
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, desktop_dc);
            return is_system_light_theme();
        }

        let bitmap = CreateCompatibleBitmap(desktop_dc, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, desktop_dc);
            return is_system_light_theme();
        }

        let old_bitmap = SelectObject(mem_dc, bitmap);

        // Capture the screen area behind the window
        let _ = BitBlt(mem_dc, 0, 0, width, height, desktop_dc, x, y, SRCCOPY);

        // Sample pixels and calculate average brightness
        // We sample a grid of points to get a representative value
        let mut total_brightness: u64 = 0;
        let mut sample_count: u64 = 0;

        let step_x = (width / 4).max(1);
        let step_y = (height / 4).max(1);

        for sy in (0..height).step_by(step_y as usize) {
            for sx in (0..width).step_by(step_x as usize) {
                let pixel = GetPixel(mem_dc, sx, sy);
                // COLORREF is a wrapper struct, access the underlying u32 value
                let pixel_value = pixel.0;
                let r = ((pixel_value >> 16) & 0xFF) as u64;
                let g = ((pixel_value >> 8) & 0xFF) as u64;
                let b = (pixel_value & 0xFF) as u64;
                // Perceived brightness formula
                let brightness = (r * 299 + g * 587 + b * 114) / 1000;
                total_brightness += brightness;
                sample_count += 1;
            }
        }

        // Cleanup
        let _ = SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, desktop_dc);

        if sample_count == 0 {
            return is_system_light_theme();
        }

        let avg_brightness = total_brightness / sample_count;
        // Threshold: 128 is middle gray, use 150 for a slightly higher threshold
        // to prefer dark icons on ambiguous backgrounds
        avg_brightness > 150
    }
}

