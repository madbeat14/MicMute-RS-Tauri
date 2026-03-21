use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};

fn main() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).unwrap();
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();

        // Get the specific devices based on the diagnostics we saw earlier
        let cap_id = "{0.0.1.00000000}.{b6eff619-2ed0-4ec1-9c18-bbc33a802301}";
        let ren_id = "{0.0.0.00000000}.{d3854aab-7c17-4841-921e-417ca4ef34dd}";

        let cap_wide: Vec<u16> = cap_id.encode_utf16().chain(std::iter::once(0)).collect();
        let ren_wide: Vec<u16> = ren_id.encode_utf16().chain(std::iter::once(0)).collect();

        let cap_dev = enumerator
            .GetDevice(windows::core::PCWSTR(cap_wide.as_ptr()))
            .unwrap();
        let ren_dev = enumerator
            .GetDevice(windows::core::PCWSTR(ren_wide.as_ptr()))
            .unwrap();

        let cap_vol: IAudioEndpointVolume = cap_dev.Activate(CLSCTX_ALL, None).unwrap();
        let ren_vol: IAudioEndpointVolume = ren_dev.Activate(CLSCTX_ALL, None).unwrap();

        let cap_mute_before = cap_vol.GetMute().unwrap();
        let ren_mute_before = ren_vol.GetMute().unwrap();

        println!("Before Mute Test:");
        println!("Capture Muted: {:?}", cap_mute_before);
        println!("Render Muted: {:?}", ren_mute_before);

        println!("\nMuting Capture device...");
        cap_vol.SetMute(true, std::ptr::null()).unwrap();

        // Add a small delay for hardware to sync
        std::thread::sleep(std::time::Duration::from_millis(500));

        let cap_mute_after = cap_vol.GetMute().unwrap();
        let ren_mute_after = ren_vol.GetMute().unwrap();

        println!("\nAfter Muting Capture device:");
        println!("Capture Muted: {:?}", cap_mute_after);
        println!("Render Muted: {:?}", ren_mute_after);

        println!("\nUnmuting Capture device...");
        cap_vol.SetMute(false, std::ptr::null()).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(500));
        let ren_mute_final = ren_vol.GetMute().unwrap();
        println!("Render Muted after Unmute: {:?}", ren_mute_final);
    }
}
