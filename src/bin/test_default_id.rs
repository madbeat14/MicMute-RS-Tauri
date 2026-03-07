use windows::Win32::Media::Audio::{eCapture, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

fn main() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).unwrap();
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();

        // Let's see what the actual default capture device is
        let dev1 = enumerator.GetDefaultAudioEndpoint(eCapture, windows::Win32::Media::Audio::eCommunications).unwrap_or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eConsole).unwrap());
        let id1 = dev1.GetId().unwrap();
        println!("Default Comms/Console Capture ID: {}", id1.to_string().unwrap_or_default());

        let dev2 = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole).unwrap();
        let id2 = dev2.GetId().unwrap();
        println!("Default Console Capture ID: {}", id2.to_string().unwrap_or_default());
    }
}
