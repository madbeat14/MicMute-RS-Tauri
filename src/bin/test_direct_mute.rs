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

        let cap_id = "{0.0.1.00000000}.{b6eff619-2ed0-4ec1-9c18-bbc33a802301}";
        let cap_wide: Vec<u16> = cap_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator
            .GetDevice(windows::core::PCWSTR(cap_wide.as_ptr()))
            .unwrap();

        let vol: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).unwrap();

        println!("Current Mute: {:?}", vol.GetMute().unwrap());

        println!("Muting directly...");
        let hr = vol.SetMute(true, std::ptr::null());
        println!("Result: {:?}", hr);

        println!("After Mute: {:?}", vol.GetMute().unwrap());
    }
}
