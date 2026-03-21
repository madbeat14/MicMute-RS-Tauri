use windows::core::Result;
use windows::Win32::Media::Audio::{eCapture, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;

fn main() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        
        let id_pwstr = device.GetId()?;
        let id_string = id_pwstr.to_string().unwrap_or_default();
        println!("Default Device ID: {}", id_string);
        
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        let is_muted = volume.GetMute()?;
        println!("Initial mute state: {:?}", is_muted);
        
        println!("Toggling mute state...");
        volume.SetMute(!is_muted, std::ptr::null())?;
        
        let new_muted = volume.GetMute()?;
        println!("New mute state: {:?}", new_muted);
        
        println!("Toggling back...");
        volume.SetMute(is_muted, std::ptr::null())?;

        Ok(())
    }
}
