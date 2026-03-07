//! Application-wide constants for MicMute-RS-Tauri
//!
//! This module contains all hardcoded values used throughout the application,
//! centralized for maintainability and clarity.

/// Default hotkey virtual key code: Media Play/Pause (0xB3)
/// Used when no hotkey is configured by the user
pub const DEFAULT_HOTKEY_VK: u32 = 0xB3;

/// Audio client buffer duration in 100-nanosecond units
/// Value of 10,000,000 = 1 second
pub const AUDIO_CLIENT_BUFFER_DURATION_100NS: i64 = 10_000_000;

/// Milliseconds per second for time conversions
pub const MS_PER_SECOND: u32 = 1000;

/// Minimum AFK timeout in seconds
pub const MIN_AFK_TIMEOUT_S: u32 = 5;

/// Maximum AFK timeout in seconds
pub const MAX_AFK_TIMEOUT_S: u32 = 3600;

/// Default AFK timeout in seconds
pub const DEFAULT_AFK_TIMEOUT_S: u32 = 60;

/// Hotkey polling interval in milliseconds
pub const HOTKEY_POLL_INTERVAL_MS: u64 = 10;

/// Default beep frequency for mute feedback (Hz)
pub const DEFAULT_BEEP_FREQ_MUTE: u32 = 650;

/// Default beep frequency for unmute feedback (Hz)
pub const DEFAULT_BEEP_FREQ_UNMUTE: u32 = 700;

/// Default beep duration in milliseconds
pub const DEFAULT_BEEP_DURATION_MS: u32 = 180;

/// Default overlay scale percentage
pub const DEFAULT_OVERLAY_SCALE: u32 = 100;

/// Maximum overlay scale percentage
pub const MAX_OVERLAY_SCALE: u32 = 500;

/// Default overlay opacity percentage
pub const DEFAULT_OVERLAY_OPACITY: u8 = 80;

/// Default OSD duration in milliseconds
pub const DEFAULT_OSD_DURATION_MS: u32 = 1500;

/// Default OSD size in pixels
pub const DEFAULT_OSD_SIZE: u32 = 150;
