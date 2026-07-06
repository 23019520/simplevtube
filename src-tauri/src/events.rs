// events.rs
//
// This file defines every event type that crosses the Rust <-> JS boundary.
// Per the architecture doc (Part C.5): this is the ONLY channel through which
// state changes propagate. audio_engine, settings_manager, and window_manager
// all emit through here; src/main.js and character-window/render.js listen here.
// No component should reach into another's internals directly.

use serde::{Deserialize, Serialize};

/// Emitted whenever the voice-activity state flips. This is what
/// character-window/render.js listens for to swap the sprite.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StateChangedEvent {
    pub state: VoiceState,
    pub timestamp_ms: u128,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceState {
    Idle,
    Talking,
}

/// Emitted on mic problems (none found, or disconnected mid-session).
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeviceErrorEvent {
    pub reason: DeviceErrorReason,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceErrorReason {
    NoDevice,
    Disconnected,
}

/// Emitted whenever settings are loaded or changed, so the Control Window UI
/// can (re)sync itself to the single source of truth in settings_manager.
/// This reflects the ACTIVE profile's settings only — see ProfilesUpdatedEvent
/// for the list of all profiles.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdatedEvent {
    pub microphone_device_id: Option<String>,
    pub sensitivity_threshold: u8,
    /// v1.2: volume readings below this (0-100 scale) are treated as silence,
    /// filtering out constant background hiss/hum before it ever reaches the
    /// sensitivity comparison.
    pub noise_gate_threshold: u8,
    /// v1.2: was a hardcoded 200ms constant; now user-adjustable.
    pub mouth_hold_time_ms: u32,
    pub idle_image_path: Option<String>,
    pub talking_image_path: Option<String>,
    pub character_window: CharacterWindowState,
    pub theme: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CharacterWindowState {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub always_on_top: bool,
    // --- v1.2 character effects (Phase 1/2 of the v2 roadmap) ---
    /// 0.0 (invisible) to 1.0 (fully opaque). Applied as CSS opacity on the
    /// sprite itself, NOT as OS window transparency — Tauri has no reliable
    /// cross-platform window-opacity API, and Windows specifically ignores
    /// alpha in the one background-color API that comes close.
    pub opacity: f32,
    /// When true, resizing is disabled at the OS window level.
    pub locked: bool,
    /// When true, mouse clicks pass through the Character Window entirely.
    pub click_through: bool,
    /// Mirrors the sprite horizontally (CSS transform, no new assets needed).
    pub flipped: bool,
    /// Degrees, -180 to 180.
    pub rotation_deg: f32,
    pub shadow_enabled: bool,
    pub outline_enabled: bool,
}

impl Default for CharacterWindowState {
    fn default() -> Self {
        Self {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 400.0,
            always_on_top: true,
            opacity: 1.0,
            locked: false,
            click_through: false,
            flipped: false,
            rotation_deg: 0.0,
            shadow_enabled: false,
            outline_enabled: false,
        }
    }
}

/// Emitted frequently (throttled) with the current mic volume level, purely
/// for driving the live intensity meter in the Control Window. Distinct
/// from StateChangedEvent, which only fires on Idle/Talking transitions.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VolumeLevelEvent {
    pub level: f32, // 0.0 - 100.0
}

/// v1.2: emitted whenever the list of profiles or the active one changes,
/// so the Control Window can populate/refresh the profile dropdown. Kept
/// separate from SettingsUpdatedEvent since that event only ever describes
/// the currently-active profile's settings, not the full list of profiles.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesUpdatedEvent {
    pub profiles: Vec<String>,
    pub active_profile: String,
}

pub const EVT_VOLUME_LEVEL: &str = "volume-level";
pub const EVT_PROFILES_UPDATED: &str = "profiles-updated";

// Event name constants — used on both the emit side (Rust) and the
// listen side (JS) so a typo can't silently create two different channels.
pub const EVT_STATE_CHANGED: &str = "state-changed";
pub const EVT_DEVICE_ERROR: &str = "device-error";
pub const EVT_SETTINGS_UPDATED: &str = "settings-updated";
