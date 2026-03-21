use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
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

        let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None).unwrap();

        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).unwrap();
        let fmt = client.GetMixFormat().unwrap();
        client
            .Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 10000000, 0, fmt, None)
            .unwrap();
        client.Start().unwrap();
        CoTaskMemFree(Some(fmt as *const _ as *const std::ffi::c_void));

        println!("Reading VU meter for 5 seconds...");
        for _ in 0..10 {
            let peak = meter.GetPeakValue().unwrap();
            println!("Peak: {}", peak);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        client.Stop().unwrap();
    }
}
