//! Monitor enumeration and change detection for multi-monitor support.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// Serializable monitor descriptor sent to the frontend.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    /// Physical X position of the monitor's top-left corner in the virtual screen.
    pub x: i32,
    /// Physical Y position of the monitor's top-left corner in the virtual screen.
    pub y: i32,
    /// Physical pixel width.
    pub width: u32,
    /// Physical pixel height.
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// Build a stable monitor identifier from name and physical resolution.
/// Format: "name|WxH" e.g. "DELL U2722D|2560x1440".
pub fn make_monitor_id(name: &str, width: u32, height: u32) -> String {
    format!("{}|{}x{}", name, width, height)
}

/// Enumerate all connected monitors using the Tauri AppHandle.
///
/// Returns an empty Vec if enumeration fails (e.g. during startup).
pub fn enumerate_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary_pos = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| *m.position());

    match app.available_monitors() {
        Ok(monitors) => monitors
            .into_iter()
            .map(|m| {
                let size = m.size();
                let pos = m.position();
                let name = m.name().map_or("Unknown", |v| v).to_string();
                let id = make_monitor_id(&name, size.width, size.height);
                let is_primary = primary_pos
                    .map(|pp| pp.x == pos.x && pp.y == pos.y)
                    .unwrap_or(false);
                MonitorInfo {
                    id,
                    name,
                    x: pos.x,
                    y: pos.y,
                    width: size.width,
                    height: size.height,
                    scale_factor: m.scale_factor(),
                    is_primary,
                }
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to enumerate monitors: {}", e);
            Vec::new()
        }
    }
}

/// Returns `true` if the monitor set has changed (different IDs or count).
pub fn monitors_changed(old: &[MonitorInfo], new: &[MonitorInfo]) -> bool {
    if old.len() != new.len() {
        return true;
    }
    let mut old_ids: Vec<&str> = old.iter().map(|m| m.id.as_str()).collect();
    let mut new_ids: Vec<&str> = new.iter().map(|m| m.id.as_str()).collect();
    old_ids.sort_unstable();
    new_ids.sort_unstable();
    old_ids != new_ids
}
