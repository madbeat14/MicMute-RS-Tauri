use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

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

pub fn get_run_on_startup() -> bool {
    let output = Command::new("schtasks")
        .args(["/Query", "/TN", "MicMuteStartup"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        out.status.success()
    } else {
        false
    }
}

pub fn set_run_on_startup(enable: bool) {
    if enable {
        create_startup_task();
    } else {
        delete_startup_task();
    }
}

fn create_startup_task() {
    let exe_path = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get current exe path for startup task");
            return;
        }
    };
    let exe_str = exe_path.to_string_lossy();

    let author = env::var("USERNAME").unwrap_or_else(|_| "Author".to_string());

    let xml_content = TASK_XML_TEMPLATE
        .replace("{AUTHOR}", &xml_escape(&author))
        .replace("{EXE_PATH}", &xml_escape(&exe_str))
        .replace("{ARGUMENTS}", "");

    let temp_dir = env::temp_dir();
    let temp_xml_path = temp_dir.join("micmute_startup.xml");

    // Write UTF-16 LE with BOM (schtasks expects this format)
    let mut utf16_bom = vec![0xFF, 0xFE];
    for c in xml_content.encode_utf16() {
        utf16_bom.push((c & 0xFF) as u8);
        utf16_bom.push((c >> 8) as u8);
    }

    let _ = fs::write(&temp_xml_path, utf16_bom);

    let path_str = temp_xml_path.to_string_lossy();

    let output = Command::new("schtasks")
        .args(["/Create", "/TN", "MicMuteStartup", "/XML", &path_str, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        if !out.status.success() {
            create_task_elevated(&path_str);
        }
    } else {
        create_task_elevated(&path_str);
    }

    let _ = fs::remove_file(temp_xml_path);
}

/// Encode a PowerShell script as a base64 UTF-16LE string for use with -EncodedCommand.
/// This avoids all shell metacharacter injection risks.
fn powershell_encoded_command(script: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    for c in script.encode_utf16() {
        let _ = buf.write_all(&c.to_le_bytes());
    }
    // base64 encode
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

fn create_task_elevated(xml_path: &str) {
    let script = format!(
        "Start-Process -FilePath 'schtasks' -ArgumentList @('/Create', '/TN', 'MicMuteStartup', '/XML', '{}', '/F') -WindowStyle Hidden -Verb RunAs -Wait",
        xml_path.replace('\'', "''")
    );
    let encoded = powershell_encoded_command(&script);
    let _ = Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
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
