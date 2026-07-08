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
    /// LEGACY (pre-v1.3): kept only so old data has somewhere to live.
    /// New code should read idle_frames/talking_frames instead. See
    /// settings_manager.rs's migration step for how these get folded in.
    pub idle_image_path: Option<String>,
    pub talking_image_path: Option<String>,
    /// v1.3: the avatar's idle/talking states can now each cycle through
    /// multiple frames (e.g. blinking, alternating mouth shapes) instead of
    /// being a single static image. A single-entry list behaves exactly
    /// like the old single-image behavior.
    pub idle_frames: Vec<String>,
    pub talking_frames: Vec<String>,
    /// Shared cycle speed for both idle_frames and talking_frames.
    pub frame_interval_ms: u32,
    pub character_window: CharacterWindowState,
    pub theme: String,
    /// v1.3: pop-up emotes, unrelated to the avatar's own idle/talking cycle.
    pub emotes: Vec<Emote>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
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

/// v1.3: a pop-up emote — a short sequence of frames shown centered on
/// screen, unrelated to (not composited onto) the avatar. "Number of
/// states" from the user's request = frame_paths.len().
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Emote {
    pub id: String,
    pub name: String,
    pub frame_paths: Vec<String>,
    /// Total on-screen time in ms. Frames are spaced evenly across this
    /// duration, then the emote disappears.
    pub duration_ms: u32,
    /// Optional Alt+<digit> local hotkey (1-9), active while the Control
    /// Window has focus. See main.js for why this isn't a system-wide
    /// global hotkey in this version.
    pub hotkey_digit: Option<u8>,
}

impl Default for Emote {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "New Emote".to_string(),
            frame_paths: Vec::new(),
            duration_ms: 1500,
            hotkey_digit: None,
        }
    }
}

/// Emitted when an emote is triggered. The Emote Window is the only
/// listener — it owns all playback/timing itself once it receives this,
/// same "dumb renderer, single event in" pattern as the Character Window.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EmoteTriggeredEvent {
    pub frame_paths: Vec<String>,
    pub duration_ms: u32,
}

pub const EVT_VOLUME_LEVEL: &str = "volume-level";
pub const EVT_PROFILES_UPDATED: &str = "profiles-updated";
pub const EVT_EMOTE_TRIGGERED: &str = "emote-triggered";

// Event name constants — used on both the emit side (Rust) and the
// listen side (JS) so a typo can't silently create two different channels.
pub const EVT_STATE_CHANGED: &str = "state-changed";
pub const EVT_DEVICE_ERROR: &str = "device-error";
pub const EVT_SETTINGS_UPDATED: &str = "settings-updated";
