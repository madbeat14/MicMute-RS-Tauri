use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};
use windows::core::Result;

fn main() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        // Let's use eCapture (1) and eRender (0)
        println!("=== CAPTURE DEVICES ===");
        let capture_collection = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let count = capture_collection.GetCount()?;
        for i in 0..count {
            if let Ok(device) = capture_collection.Item(i) {
                if let Ok(id) = device.GetId() {
                    let id_str = id.to_string().unwrap_or_default();
                    let name = get_device_name(&device);
                    if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                        let is_muted = vol.GetMute().unwrap_or_default().as_bool();
                        println!(" - [{}] '{}' MUTED: {}", id_str, name, is_muted);
                    }
                }
            }
        }

        println!("\n=== RENDER DEVICES ===");
        let render_collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let count = render_collection.GetCount()?;
        for i in 0..count {
            if let Ok(device) = render_collection.Item(i) {
                if let Ok(id) = device.GetId() {
                    let id_str = id.to_string().unwrap_or_default();
                    let name = get_device_name(&device);
                    if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                        let is_muted = vol.GetMute().unwrap_or_default().as_bool();
                        println!(" - [{}] '{}' MUTED: {}", id_str, name, is_muted);
                    }
                }
            }
        }
    }
    Ok(())
}

fn get_device_name(device: &windows::Win32::Media::Audio::IMMDevice) -> String {
    unsafe {
        let mut name = "Unknown".to_string();
        if let Ok(store) = device.OpenPropertyStore(STGM_READ) {
            if let Ok(prop_var) = store.GetValue(&PKEY_Device_FriendlyName) {
                let ptr = &prop_var as *const _ as *const u16;
                let vt = *ptr;
                if vt == 31 {
                    let wstr_ptr_addr = ptr.add(4) as *const *const u16;
                    let wstr_ptr = *wstr_ptr_addr;
                    if !wstr_ptr.is_null() {
                        let mut len = 0;
                        while *wstr_ptr.add(len) != 0 {
                            len += 1;
                        }
                        let slice = std::slice::from_raw_parts(wstr_ptr, len);
                        name = String::from_utf16_lossy(slice);
                    }
                }
            }
        }
        name
    }
}
