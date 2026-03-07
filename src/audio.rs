//! Audio device control and feedback playback.
//!
//! This module provides [`AudioController`] for managing microphone mute state
//! and audio feedback (beeps/custom sounds). It uses Windows Core Audio APIs
//! via COM interfaces.
//!
//! # Example
//! ```no_run
//! let controller = AudioController::new(None)?;
//! let is_muted = controller.is_muted()?;
//! controller.set_mute(true, &config)?;
//! ```

use crate::config::AppConfig;
use crate::constants::AUDIO_CLIENT_BUFFER_DURATION_100NS;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source, source::SineWave};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::time::Duration;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    eConsole,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};
use windows::core::Result;

const MUTE_WAV: &[u8] = include_bytes!("../ui/assets/mute.wav");
const UNMUTE_WAV: &[u8] = include_bytes!("../ui/assets/unmute.wav");

/// Manages audio device control including mute state and peak metering.
///
/// This struct holds COM interfaces to the Windows Core Audio APIs.
/// The [`device`] and [`audio_client`] fields are kept alive to maintain
/// COM references required by the [`volume`] and [`meter`] interfaces.
pub struct AudioController {
    /// Kept alive to maintain COM reference, but not accessed directly
    /// after construction. Volume and meter interfaces depend on this.
    _device: IMMDevice,
    volume: IAudioEndpointVolume,
    meter: IAudioMeterInformation,
    /// Audio client must be kept alive to maintain hardware streaming
    /// for peak meter readings.
    _audio_client: Option<IAudioClient>,
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
}

impl AudioController {
    pub fn new(device_id: Option<&String>) -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default().map_err(|e| {
            windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                format!("Failed to initialize audio output: {}", e),
            )
        })?;

        unsafe {
            // Ensure COM is initialized for the thread
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let device: IMMDevice = if let Some(id) = device_id {
                let wide_id: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                enumerator.GetDevice(windows::core::PCWSTR(wide_id.as_ptr()))?
            } else {
                enumerator
                    .GetDefaultAudioEndpoint(windows::Win32::Media::Audio::eCapture, eConsole)?
            };

            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None)?;

            let mut audio_client = None;
            if let Ok(client) = device.Activate::<IAudioClient>(CLSCTX_ALL, None) {
                if let Ok(fmt) = client.GetMixFormat() {
                    // Initialize and Start the client so the hardware starts feeding meter data
                    // AUDCLNT_SHAREMODE_SHARED = 0
                    if client
                        .Initialize(AUDCLNT_SHAREMODE_SHARED, 0, AUDIO_CLIENT_BUFFER_DURATION_100NS, 0, fmt, None)
                        .is_ok()
                    {
                        if client.Start().is_ok() {
                            audio_client = Some(client);
                        }
                    } else {
                        tracing::error!("Failed to initialize AudioClient");
                    }
                    windows::Win32::System::Com::CoTaskMemFree(Some(
                        fmt as *const _ as *const std::ffi::c_void,
                    ));
                }
            }

            Ok(Self {
                _device: device,
                volume,
                meter,
                _audio_client: audio_client,
                _stream,
                stream_handle,
            })
        }
    }

    pub fn is_muted(&self) -> Result<bool> {
        let muted = unsafe { self.volume.GetMute() }.map_err(|e| {
            tracing::error!(error = ?e, "GetMute failed");
            e
        })?;
        Ok(muted.as_bool())
    }

    pub fn set_mute(&self, mute: bool, config: &AppConfig) -> Result<String> {
        let mut debug_msg = String::new();
        if let Err(e) = unsafe { self.volume.SetMute(mute, std::ptr::null()) } {
            tracing::error!(error = ?e, mute = mute, "Failed to set mute state");
            return Err(e);
        }
        debug_msg.push_str(&format!("Muted Main: {}; ", mute));

        // Sync logic mirroring python implementation
        if !config.sync_ids.is_empty() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                if let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(
                    &MMDeviceEnumerator,
                    None,
                    CLSCTX_ALL,
                ) {
                    if let Ok(collection) = enumerator.EnumAudioEndpoints(
                        windows::Win32::Media::Audio::eCapture,
                        windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE,
                    ) {
                        if let Ok(count) = collection.GetCount() {
                            for i in 0..count {
                                if let Ok(dev) = collection.Item(i) {
                                    if let Ok(id_pwstr) = dev.GetId() {
                                        let id_string = id_pwstr.to_string().unwrap_or_default();
                                        if let Some(main_id) = &config.device_id {
                                            if &id_string == main_id {
                                                continue;
                                            }
                                        }
                                        if config.sync_ids.contains(&id_string) {
                                            if let Ok(vol) = dev
                                                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                                            {
                                                if let Err(e) = vol.SetMute(mute, std::ptr::null())
                                                {
                                                    tracing::error!(
                                                        device_id = %id_string,
                                                        error = ?e,
                                                        "Failed to set mute state for sync device"
                                                    );
                                                } else {
                                                    debug_msg.push_str(&format!(
                                                        "Sync {}: {}; ",
                                                        id_string, mute
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(debug_msg)
    }

    pub fn toggle_mute(&self, config: &AppConfig) -> Result<(bool, String)> {
        let current = self.is_muted()?;
        let new_state = !current;
        let mut debug = self.set_mute(new_state, config)?;
        debug.push_str(&format!("Current is_muted() reads: {}", new_state));
        Ok((new_state, debug))
    }

    pub fn get_peak_value(&self) -> Result<f32> {
        let peak = unsafe { self.meter.GetPeakValue() }.map_err(|e| {
            tracing::error!(error = ?e, "GetPeakValue failed");
            e
        })?;
        Ok(peak)
    }

    pub fn stream_handle(&self) -> OutputStreamHandle {
        self.stream_handle.clone()
    }
}

pub fn play_feedback(stream_handle: &OutputStreamHandle, is_muted: bool, config: &AppConfig) {
    if !config.beep_enabled {
        return;
    }

    let key = if is_muted { "mute" } else { "unmute" };

    if config.audio_mode == "beep" {
        if let Some(beep_cfg) = config.beep_mode_configs.get(key) {
            let sink = Sink::try_new(stream_handle).unwrap();
            for _ in 0..beep_cfg.count {
                let source = SineWave::new(beep_cfg.freq as f32)
                    .take_duration(Duration::from_millis(beep_cfg.duration as u64))
                    .amplify(0.2);
                sink.append(source);
            }
            sink.detach();
        }
    } else {
        // "custom" mode
        if let Some(sound_cfg) = config.sound_mode_configs.get(key) {
            let mut path_found = None;
            let sound_cfg_file = &sound_cfg.file;

            let p = std::path::PathBuf::from(sound_cfg_file);
            if p.is_absolute() && p.exists() {
                path_found = Some(p);
            } else {
                // Check local assets (Priority for Rust version)
                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(parent) = exe_path.parent() {
                        let local_assets = parent.join("ui").join("assets").join(sound_cfg_file);
                        if local_assets.exists() {
                            path_found = Some(local_assets);
                        }
                    }
                }
                if path_found.is_none() {
                    let cwd_assets = std::env::current_dir()
                        .unwrap_or_default()
                        .join("ui")
                        .join("assets")
                        .join(sound_cfg_file);
                    if cwd_assets.exists() {
                        path_found = Some(cwd_assets);
                    }
                }
                if path_found.is_none() {
                    // Fallback to Python AppData sounds directory
                    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "MicMute") {
                        let appdata_path = proj_dirs
                            .data_local_dir()
                            .parent()
                            .unwrap_or(proj_dirs.data_local_dir())
                            .join("MicMute")
                            .join("micmute_sounds")
                            .join(sound_cfg_file);
                        if appdata_path.exists() {
                            path_found = Some(appdata_path);
                        }
                    }
                }
            }

            if let Some(valid_path) = path_found {
                if let Ok(file) = File::open(&valid_path) {
                    if let Ok(source) = rodio::Decoder::new(BufReader::new(file)) {
                        let sink = Sink::try_new(stream_handle).unwrap();
                        sink.set_volume((sound_cfg.volume as f32) / 100.0);
                        sink.append(source);
                        sink.detach();
                    } else {
                        tracing::error!(path = ?valid_path, "Failed to decode audio file");
                    }
                } else {
                    tracing::error!(path = ?valid_path, "Failed to open audio file");
                }
            } else {
                tracing::error!(
                    file = %sound_cfg_file,
                    "Audio file not found, using embedded fallback"
                );

                let bytes = if key == "mute" { MUTE_WAV } else { UNMUTE_WAV };
                if let Ok(source) = rodio::Decoder::new(Cursor::new(bytes)) {
                    let sink = Sink::try_new(stream_handle).unwrap();
                    sink.set_volume((sound_cfg.volume as f32) / 100.0);
                    sink.append(source);
                    sink.detach();
                } else {
                    // Final fallback to beep if even embedded decode fails (shouldn't happen)
                    if let Some(beep_cfg) = config.beep_mode_configs.get(key) {
                        let sink = Sink::try_new(stream_handle).unwrap();
                        let source = SineWave::new(beep_cfg.freq as f32)
                            .take_duration(Duration::from_millis(beep_cfg.duration as u64))
                            .amplify(0.2);
                        sink.append(source);
                        sink.detach();
                    }
                }
            }
        }
    }
}
pub fn get_audio_devices() -> Result<Vec<(String, String)>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let collection = enumerator.EnumAudioEndpoints(
            windows::Win32::Media::Audio::eCapture,
            windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE,
        )?;
        let count = collection.GetCount()?;
        let mut devices = Vec::new();

        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                if let Ok(id_pwstr) = device.GetId() {
                    let id_string = id_pwstr.to_string().unwrap_or_default();
                    let mut name = "Unknown Device".to_string();

                    if let Ok(store) = device.OpenPropertyStore(STGM_READ) {
                        if let Ok(prop_var) = store.GetValue(&PKEY_Device_FriendlyName) {
                            let ptr = &prop_var as *const _ as *const u16;
                            let vt = *ptr;
                            if vt == 31 {
                                // VT_LPWSTR
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
                            } else {
                                name = id_string.clone();
                            }
                        }
                    }
                    devices.push((id_string, name));
                }
            }
        }
        Ok(devices)
    }
}
