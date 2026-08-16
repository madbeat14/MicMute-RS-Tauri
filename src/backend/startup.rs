//! Windows autostart configuration via Task Scheduler and Registry Run key.
//!
//! Provides defense-in-depth: attempts high-priority Task Scheduler registration first,
//! with automatic graceful fallback to HKCU Registry Run Key if Task Scheduler is
//! unavailable or blocked by enterprise policy.

use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const REG_RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_REG_NAME: &str = "MicMuteRs";

/// Escape special characters for safe XML interpolation.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const TASK_XML_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>{AUTHOR}</Author>
    <Description>Start MicMute at startup with High Priority</Description>
    <URI>\MicMuteStartup</URI>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>true</StopIfGoingOnBatteries>
    <AllowHardTerminate>false</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>true</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <Priority>0</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{EXE_PATH}</Command>
      <Arguments>{ARGUMENTS}</Arguments>
    </Exec>
  </Actions>
</Task>"#;

/// Query whether startup is enabled via Task Scheduler or Registry Run Key.
pub fn get_run_on_startup() -> bool {
    let task_exists = Command::new("schtasks")
        .args(["/Query", "/TN", "MicMuteStartup"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if task_exists {
        return true;
    }

    get_registry_run()
}

/// Enable or disable startup across both mechanisms.
pub fn set_run_on_startup(enable: bool) {
    if enable {
        let task_ok = create_startup_task();
        if !task_ok {
            tracing::info!("Task Scheduler creation failed/declined; falling back to HKCU Registry Run Key");
            set_registry_run(true);
        }
    } else {
        delete_startup_task();
        set_registry_run(false);
    }
}

fn create_startup_task() -> bool {
    let exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get current exe path for startup task");
            return false;
        }
    };
    let exe_str = exe_path.to_string_lossy();

    let author = env::var("USERNAME").unwrap_or_else(|_| "Author".to_string());

    let xml_content = TASK_XML_TEMPLATE
        .replace("{AUTHOR}", &xml_escape(&author))
        .replace("{EXE_PATH}", &xml_escape(&exe_str))
        .replace("{ARGUMENTS}", "");

    let temp_dir = env::temp_dir();
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_xml_path = temp_dir.join(format!("micmute_startup_{}_{}.xml", std::process::id(), unique_id));

    // Write UTF-16 LE with BOM (schtasks expects this format)
    let mut utf16_bom = vec![0xFF, 0xFE];
    for c in xml_content.encode_utf16() {
        utf16_bom.push((c & 0xFF) as u8);
        utf16_bom.push((c >> 8) as u8);
    }

    if fs::write(&temp_xml_path, utf16_bom).is_err() {
        return false;
    }

    let path_str = temp_xml_path.to_string_lossy();

    let output = Command::new("schtasks")
        .args(["/Create", "/TN", "MicMuteStartup", "/XML", &path_str, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let success = if let Ok(out) = output {
        if out.status.success() {
            true
        } else {
            create_task_elevated(&path_str)
        }
    } else {
        create_task_elevated(&path_str)
    };

    let _ = fs::remove_file(temp_xml_path);
    success
}

/// Encode a PowerShell script as a base64 UTF-16LE string for use with -EncodedCommand.
fn powershell_encoded_command(script: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    for c in script.encode_utf16() {
        let _ = buf.write_all(&c.to_le_bytes());
    }
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in buf.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn create_task_elevated(xml_path: &str) -> bool {
    let script = format!(
        "Start-Process -FilePath 'schtasks' -ArgumentList @('/Create', '/TN', 'MicMuteStartup', '/XML', '{}', '/F') -WindowStyle Hidden -Verb RunAs -Wait",
        xml_path.replace('\'', "''")
    );
    let encoded = powershell_encoded_command(&script);
    Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn delete_startup_task() {
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", "MicMuteStartup", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        if !out.status.success() {
            delete_task_elevated();
        }
    } else {
        delete_task_elevated();
    }
}

fn delete_task_elevated() {
    let script = "Start-Process -FilePath 'schtasks' -ArgumentList @('/Delete', '/TN', 'MicMuteStartup', '/F') -WindowStyle Hidden -Verb RunAs -Wait";
    let encoded = powershell_encoded_command(script);
    let _ = Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

// ─────────────────────────────────────────
//  Registry Run Key Fallback
// ─────────────────────────────────────────

fn get_registry_run() -> bool {
    let subkey: Vec<u16> = format!("{REG_RUN_SUBKEY}\0").encode_utf16().collect();
    let val_name: Vec<u16> = format!("{APP_REG_NAME}\0").encode_utf16().collect();

    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_ok()
        {
            let mut data_size: u32 = 0;
            let res = RegQueryValueExW(
                hkey,
                PCWSTR(val_name.as_ptr()),
                None,
                None,
                None,
                Some(&mut data_size),
            );
            let _ = RegCloseKey(hkey);
            return res.is_ok() && data_size > 0;
        }
    }
    false
}

fn set_registry_run(enable: bool) {
    let subkey: Vec<u16> = format!("{REG_RUN_SUBKEY}\0").encode_utf16().collect();
    let val_name: Vec<u16> = format!("{APP_REG_NAME}\0").encode_utf16().collect();

    unsafe {
        let mut hkey = Default::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
        .is_ok()
        {
            if enable {
                if let Ok(exe) = env::current_exe() {
                    let exe_str = format!("\"{}\"\0", exe.display());
                    let wide_val: Vec<u16> = exe_str.encode_utf16().collect();
                    let _ = RegSetValueExW(
                        hkey,
                        PCWSTR(val_name.as_ptr()),
                        0,
                        REG_SZ,
                        Some(std::slice::from_raw_parts(
                            wide_val.as_ptr() as *const u8,
                            wide_val.len() * 2,
                        )),
                    );
                }
            } else {
                let _ = RegDeleteValueW(hkey, PCWSTR(val_name.as_ptr()));
            }
            let _ = RegCloseKey(hkey);
        }
    }
}
