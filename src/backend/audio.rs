//! Audio device control and feedback playback.
//!
//! This module provides [`AudioController`] for managing microphone mute state
//! and audio feedback (beeps/custom sounds). It uses Windows Core Audio APIs
//! via COM interfaces.
//!
//! # Example
//! ```ignore
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

/// RAII guard for COM-allocated memory that must be freed with `CoTaskMemFree`.
/// Prevents memory leaks if an early return or panic occurs before manual free.
struct CoTaskMemGuard<T>(*mut T);

impl<T> Drop for CoTaskMemGuard<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows::Win32::System::Com::CoTaskMemFree(Some(self.0 as *const std::ffi::c_void));
            }
        }
    }
}

use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    eConsole,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
};
use windows::core::Result;

const MUTE_WAV: &[u8] = include_bytes!("../frontend/assets/mute.wav");
const UNMUTE_WAV: &[u8] = include_bytes!("../frontend/assets/unmute.wav");

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
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
}

impl AudioController {
    pub fn new(device_id: Option<&String>) -> Result<Self> {
        let (opt_stream, opt_handle) = match OutputStream::try_default() {
            Ok((_stream, stream_handle)) => (Some(_stream), Some(stream_handle)),
            Err(e) => {
                tracing::warn!(error = %e, "Audio output unavailable; beep/WAV feedback disabled");
                (None, None)
            }
        };

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
            if let Ok(client) = device.Activate::<IAudioClient>(CLSCTX_ALL, None)
                && let Ok(fmt) = client.GetMixFormat() {
                    // SAFETY: `fmt` was allocated by COM via GetMixFormat.
                    // Guard ensures it is freed on scope exit regardless of control flow.
                    let _fmt_guard = CoTaskMemGuard(fmt);

                    // Initialize and Start the client so the hardware starts feeding meter data
                    if client
                        .Initialize(
                            AUDCLNT_SHAREMODE_SHARED,
                            0,
                            AUDIO_CLIENT_BUFFER_DURATION_100NS,
                            0,
                            fmt,
                            None,
                        )
                        .is_ok()
                    {
                        if client.Start().is_ok() {
                            audio_client = Some(client);
                        }
                    } else {
                        tracing::error!("Failed to initialize AudioClient");
                    }
                }

            Ok(Self {
                _device: device,
                volume,
                meter,
                _audio_client: audio_client,
                _stream: opt_stream,
                stream_handle: opt_handle,
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

    pub fn set_mute(&self, mute: bool, config: &AppConfig) -> Result<()> {
        if let Err(e) = unsafe { self.volume.SetMute(mute, std::ptr::null()) } {
            tracing::error!(error = ?e, mute = mute, "Failed to set mute state");
            return Err(e);
        }
        tracing::debug!(mute = mute, "Set mute on main device");

        if !config.sync_ids.is_empty() {
            self.sync_mute_to_devices(mute, config);
        }
        Ok(())
    }

    /// Apply mute state to all configured sync devices, skipping the main device.
    fn sync_mute_to_devices(&self, mute: bool, config: &AppConfig) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(enumerator) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            else {
                return;
            };
            let Ok(collection) = enumerator.EnumAudioEndpoints(
                windows::Win32::Media::Audio::eCapture,
                windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE,
            ) else {
                return;
            };
            let Ok(count) = collection.GetCount() else {
                return;
            };

            for i in 0..count {
                let Ok(dev) = collection.Item(i) else {
                    continue;
                };
                let Ok(id_pwstr) = dev.GetId() else { continue };
                let _id_guard = CoTaskMemGuard(id_pwstr.0);
                let id_string = match id_pwstr.to_string() {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = ?e, "Failed to convert device ID to string");
                        continue;
                    }
                };

                if config.device_id.as_ref() == Some(&id_string) {
                    continue;
                }
                if !config.sync_ids.contains(&id_string) {
                    continue;
                }

                let Ok(vol) = dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) else {
                    continue;
                };
                if let Err(e) = vol.SetMute(mute, std::ptr::null()) {
                    tracing::error!(
                        device_id = %id_string,
                        error = ?e,
                        "Failed to set mute state for sync device"
                    );
                } else {
                    tracing::debug!(
                        device_id = %id_string,
                        mute = mute,
                        "Synced mute state"
                    );
                }
            }
        }
    }

    pub fn toggle_mute(&self, config: &AppConfig) -> Result<bool> {
        let current = self.is_muted()?;
        let new_state = !current;
        self.set_mute(new_state, config)?;
        Ok(new_state)
    }

    pub fn get_peak_value(&self) -> Result<f32> {
        let peak = unsafe { self.meter.GetPeakValue() }.map_err(|e| {
            tracing::error!(error = ?e, "GetPeakValue failed");
            e
        })?;
        Ok(peak)
    }

    pub fn stream_handle(&self) -> Option<OutputStreamHandle> {
        self.stream_handle.clone()
    }
}

/// Reject paths containing `..` components to prevent path traversal.
fn is_safe_sound_path(path: &str) -> bool {
    !std::path::Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Play audio feedback and return the Sink so the caller can keep it alive.
/// When the returned Sink is dropped, playback stops immediately — this lets
/// the worker thread cancel a previous sound by simply replacing it.
pub fn play_feedback(
    stream_handle: Option<&OutputStreamHandle>,
    is_muted: bool,
    config: &AppConfig,
) -> Option<Sink> {
    if !config.beep_enabled {
        return None;
    }
    let stream_handle = stream_handle?;

    let key = if is_muted { "mute" } else { "unmute" };

    if config.audio_mode == "beep" {
        if let Some(beep_cfg) = config.beep_mode_configs.get(key)
            && let Ok(sink) = Sink::try_new(stream_handle) {
                for _ in 0..beep_cfg.count {
                    let source = SineWave::new(beep_cfg.freq as f32)
                        .take_duration(Duration::from_millis(beep_cfg.duration as u64))
                        .amplify(0.2);
                    sink.append(source);
                }
                return Some(sink);
            }
    } else {
        // "custom" mode
        if let Some(sound_cfg) = config.sound_mode_configs.get(key) {
            let mut path_found = None;
            let sound_cfg_file = &sound_cfg.file;

            if !is_safe_sound_path(sound_cfg_file) {
                tracing::error!(file = %sound_cfg_file, "Sound path rejected: contains path traversal");
                return None;
            }

            let p = std::path::PathBuf::from(sound_cfg_file);
            if p.is_absolute() && p.exists() {
                path_found = Some(p);
            } else {
                // Check local assets (Priority for Rust version)
                if let Ok(exe_path) = std::env::current_exe()
                    && let Some(parent) = exe_path.parent() {
                        let local_assets = parent
                            .join("src")
                            .join("frontend")
                            .join("assets")
                            .join(sound_cfg_file);
                        if local_assets.exists() {
                            path_found = Some(local_assets);
                        }
                    }
                if path_found.is_none() {
                    let cwd_assets = std::env::current_dir()
                        .unwrap_or_default()
                        .join("src")
                        .join("frontend")
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

            let volume = (sound_cfg.volume as f32) / 100.0;

            if let Some(valid_path) = path_found {
                if let Ok(file) = File::open(&valid_path) {
                    if let Ok(source) = rodio::Decoder::new(BufReader::new(file)) {
                        if let Ok(sink) = Sink::try_new(stream_handle) {
                            sink.set_volume(volume);
                            sink.append(source);
                            return Some(sink);
                        }
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
                    if let Ok(sink) = Sink::try_new(stream_handle) {
                        sink.set_volume(volume);
                        sink.append(source);
                        return Some(sink);
                    }
                } else if let Some(beep_cfg) = config.beep_mode_configs.get(key)
                    && let Ok(sink) = Sink::try_new(stream_handle) {
                        let source = SineWave::new(beep_cfg.freq as f32)
                            .take_duration(Duration::from_millis(beep_cfg.duration as u64))
                            .amplify(0.2);
                        sink.append(source);
                        return Some(sink);
                    }
            }
        }
    }
    None
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
            if let Ok(device) = collection.Item(i)
                && let Ok(id_pwstr) = device.GetId() {
                    let _id_guard = CoTaskMemGuard(id_pwstr.0);
                    let id_string = match id_pwstr.to_string() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to convert device ID to string");
                            continue;
                        }
                    };
                    let mut name = "Unknown Device".to_string();

                    if let Ok(store) = device.OpenPropertyStore(STGM_READ)
                        && let Ok(prop_var) = store.GetValue(&PKEY_Device_FriendlyName) {
                            // SAFETY: We check the variant type (vt) equals VT_LPWSTR (31)
                            // before accessing the union's string pointer. When vt == VT_LPWSTR,
                            // the PROPVARIANT data at offset 8 is a valid LPWSTR pointer.
                            // SAFETY: vt is at offset 0 of PROPVARIANT (u16 discriminant).
                            // We only access the string pointer when vt == VT_LPWSTR.
                            // All pointer operations are within the outer unsafe block.
                            use windows::Win32::System::Variant::VT_LPWSTR;
                            let vt = (*(&prop_var as *const _ as *const u16)) as u32;
                            let name_str = if vt == VT_LPWSTR.0 as u32 {
                                let ptr = &prop_var as *const _ as *const u64;
                                let pwstr_ptr = *(ptr.add(1) as *const *const u16);
                                if !pwstr_ptr.is_null() {
                                    let name_pwstr = windows::core::PWSTR(pwstr_ptr as *mut _);
                                    name_pwstr.to_string().unwrap_or_else(|_| id_string.clone())
                                } else {
                                    id_string.clone()
                                }
                            } else {
                                tracing::warn!(vt = vt, "PROPVARIANT is not VT_LPWSTR, skipping name extraction");
                                id_string.clone()
                            };
                            name = name_str;
                        }
                    devices.push((id_string, name));
                }
        }
        Ok(devices)
    }
}
