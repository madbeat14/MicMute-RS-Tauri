use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::Result;

fn main() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None)?;

        println!("Setting Mute to TRUE...");
        volume.SetMute(true, std::ptr::null())?;
        println!("Mute State is now: {:?}", volume.GetMute()?);

        for _i in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            println!("Peak (Muted): {}", meter.GetPeakValue()?);
        }

        println!("Setting Mute to FALSE...");
        volume.SetMute(false, std::ptr::null())?;
        println!("Mute State is now: {:?}", volume.GetMute()?);

        for _i in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            println!("Peak (Unmuted): {}", meter.GetPeakValue()?);
        }
    }
    Ok(())
}
