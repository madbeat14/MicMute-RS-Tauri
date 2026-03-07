use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

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
        .args(&["/Query", "/TN", "MicMuteStartup"])
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
    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("MicMuteRs.exe"));
    let exe_str = exe_path.to_string_lossy();
    // In Rust we don't necessarily need quotes around args if we don't have spaces, but good practice
    let _arguments = format!("\"{}\"", exe_str);

    let author = env::var("USERNAME").unwrap_or_else(|_| "Author".to_string());

    let xml_content = TASK_XML_TEMPLATE
        .replace("{AUTHOR}", &author)
        .replace("{EXE_PATH}", &exe_str)
        .replace("{ARGUMENTS}", ""); // Empty args for now as exe_path itself is the target

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
        .args(&["/Create", "/TN", "MicMuteStartup", "/XML", &path_str, "/F"])
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

fn create_task_elevated(xml_path: &str) {
    let schtasks_args = format!("/Create /TN \"MicMuteStartup\" /XML \"{}\" /F", xml_path);
    let _ = Command::new("powershell")
        .args(&[
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "Start-Process schtasks -ArgumentList '{}' -WindowStyle Hidden -Verb RunAs -Wait",
                schtasks_args
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

fn delete_startup_task() {
    let output = Command::new("schtasks")
        .args(&["/Delete", "/TN", "MicMuteStartup", "/F"])
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
    let schtasks_args = "/Delete /TN \"MicMuteStartup\" /F";
    let _ = Command::new("powershell")
        .args(&[
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "Start-Process schtasks -ArgumentList '{}' -WindowStyle Hidden -Verb RunAs -Wait",
                schtasks_args
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}
