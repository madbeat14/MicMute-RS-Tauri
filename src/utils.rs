use windows::Win32::System::Registry::{RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER};
use windows::Win32::System::Registry::KEY_READ;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::System::SystemInformation::GetTickCount;

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

