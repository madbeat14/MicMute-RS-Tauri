//! Utility functions: idle detection, VK code mapping, window helpers.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

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

/// Toggle WS_EX_TRANSPARENT on an HWND without touching other extended styles.
/// Tauri's set_ignore_cursor_events() rebuilds ALL extended styles via
/// SetWindowLongW, which removes WS_EX_LAYERED and breaks transparent
/// window compositing. This function only toggles the one bit we need.
pub fn set_click_through(hwnd: HWND, click_through: bool) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let new_ex_style = if click_through {
            ex_style | WS_EX_TRANSPARENT.0
        } else {
            ex_style & !WS_EX_TRANSPARENT.0
        };
        if new_ex_style != ex_style {
            SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style as i32);
        }
    }
}

/// Force a window to HWND_TOPMOST z-order using SetWindowPos directly.
/// More reliable than Tauri's set_always_on_top because it issues the
/// Win32 call without rebuilding window styles.
pub fn force_topmost(hwnd: HWND) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .ok();
    }
}
