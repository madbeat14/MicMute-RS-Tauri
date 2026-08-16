//! Utility functions: idle detection, VK code mapping, window helpers.

use std::borrow::Cow;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetWindowLongPtrW, SetWindowPos, WS_EX_TRANSPARENT,
};

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

pub fn vk_to_string(vk: u32) -> Cow<'static, str> {
    match vk {
        0 => Cow::Borrowed("None"),
        0x08 => Cow::Borrowed("Backspace"),
        0x09 => Cow::Borrowed("Tab"),
        0x0D => Cow::Borrowed("Enter"),
        0x10 => Cow::Borrowed("Shift"),
        0x11 => Cow::Borrowed("Ctrl"),
        0x12 => Cow::Borrowed("Alt"),
        0x13 => Cow::Borrowed("Pause"),
        0x14 => Cow::Borrowed("Caps Lock"),
        0x1B => Cow::Borrowed("Esc"),
        0x20 => Cow::Borrowed("Space"),
        0x30..=0x39 => Cow::Owned(format!("{}", (vk - 0x30) as u8)),
        0x41..=0x5A => Cow::Owned(format!("{}", ((vk - 0x41) as u8 + b'A') as char)),
        0x60..=0x69 => Cow::Owned(format!("Numpad {}", (vk - 0x60) as u8)),
        0x70..=0x87 => Cow::Owned(format!("F{}", (vk - 0x70) + 1)),
        0xA0 => Cow::Borrowed("LShift"),
        0xA1 => Cow::Borrowed("RShift"),
        0xA2 => Cow::Borrowed("LCtrl"),
        0xA3 => Cow::Borrowed("RCtrl"),
        0xA4 => Cow::Borrowed("LAlt"),
        0xA5 => Cow::Borrowed("RAlt"),
        0xAF => Cow::Borrowed("Volume Up"),
        0xAE => Cow::Borrowed("Volume Down"),
        0xAD => Cow::Borrowed("Volume Mute"),
        0xB0 => Cow::Borrowed("Media Next"),
        0xB1 => Cow::Borrowed("Media Prev"),
        0xB2 => Cow::Borrowed("Media Stop"),
        0xB3 => Cow::Borrowed("Media Play/Pause"),
        _ => Cow::Owned(format!("VK_0x{:02X}", vk)),
    }
}

/// Toggle WS_EX_TRANSPARENT on an HWND without touching other extended styles.
/// Tauri's set_ignore_cursor_events() rebuilds ALL extended styles via
/// SetWindowLong, which removes WS_EX_LAYERED and breaks transparent
/// window compositing. This function only toggles the single bit needed.
pub fn set_click_through(hwnd: HWND, click_through: bool) {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let new_ex_style = if click_through {
            ex_style | WS_EX_TRANSPARENT.0
        } else {
            ex_style & !WS_EX_TRANSPARENT.0
        };
        if new_ex_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style as isize);
        }
    }
}

/// Force a window to HWND_TOPMOST z-order using SetWindowPos directly.
/// More reliable than Tauri's set_always_on_top because it issues the
/// Win32 call without rebuilding window styles.
pub fn force_topmost(hwnd: HWND) {
    unsafe {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vk_to_string_names() {
        assert_eq!(vk_to_string(0), "None");
        assert_eq!(vk_to_string(0x20), "Space");
        assert_eq!(vk_to_string(0xB3), "Media Play/Pause");
        assert_eq!(vk_to_string(0x41), "A");
        assert_eq!(vk_to_string(0x70), "F1");
        assert_eq!(vk_to_string(0x60), "Numpad 0");
        assert_eq!(vk_to_string(0xFF), "VK_0xFF");
    }
}

