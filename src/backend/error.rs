//! Application error types and Tauri IPC error formatting.

use serde::Serialize;
use thiserror::Error;

/// Structured command errors returned across the Tauri IPC bridge.
#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Failed to persist configuration: {0}")]
    ConfigSave(String),
    #[error("Audio worker channel disconnected")]
    AudioWorkerUnavailable,
    #[error("Invalid parameter: {0}")]
    InvalidInput(String),
    #[error("Native Windows error: {0}")]
    Win32(String),
    #[error("Dialog error: {0}")]
    Dialog(String),
    #[error("Payload exceeds maximum size")]
    PayloadTooLarge,
    #[error("{0}")]
    Message(String),
}

impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        CommandError::Message(s)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        CommandError::Message(s.to_string())
    }
}
