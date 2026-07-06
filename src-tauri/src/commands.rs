// commands.rs
//
// Every #[tauri::command] here corresponds exactly to a row in the
// architecture doc's C.3 "File-to-File Contracts" table. src/main.js calls
// these via invoke(); nothing else calls these directly.
//
// v1.2 adds three groups of commands on top of the original set:
//   - Phase 1 (quick wins): noise gate, hold time, opacity, lock,
//     click-through, flip, drag-and-drop image loading, scaling hotkeys
//   - Phase 2 (presentation): shadow, outline, rotation
//   - Phase 4 (profiles): list/create/switch/delete

use crate::audio_engine::AudioEngine;
use crate::events::{ProfilesUpdatedEvent, SettingsUpdatedEvent, EVT_PROFILES_UPDATED, EVT_SETTINGS_UPDATED};
use crate::settings_manager::SettingsManager;
use crate::window_manager::WindowManager;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

pub struct AppState {
    pub settings: Arc<SettingsManager>,
    pub audio: Arc<AudioEngine>,
    pub windows: Arc<WindowManager>,
}

#[derive(Serialize)]
pub struct MicrophoneOption {
    id: String,
    name: String,
}

fn broadcast_settings(app: &AppHandle, settings: &SettingsManager) {
    let event: SettingsUpdatedEvent = (&settings.get()).into();
    let _ = app.emit(EVT_SETTINGS_UPDATED, event);
}

fn broadcast_profiles(app: &AppHandle, settings: &SettingsManager) {
    let (profiles, active_profile) = settings.list_profiles();
    let _ = app.emit(EVT_PROFILES_UPDATED, ProfilesUpdatedEvent { profiles, active_profile });
}

/// Called once from main.rs's setup() to fire the initial settings-updated
/// and profiles-updated events (boot sequence step 2->3 in Architecture C.4).
pub fn broadcast_initial_settings(app: &AppHandle, settings: &SettingsManager) {
    broadcast_settings(app, settings);
    broadcast_profiles(app, settings);
}

/// Shared validation+persistence logic for setting an idle/talking image,
/// used by BOTH the file-dialog flow (select_idle_image/select_talking_image)
/// and the drag-and-drop flow (set_image_from_dropped_path) — one place
/// that decides "is this a valid image" and "how do we store it".
fn validate_and_store_image(
    app: &AppHandle,
    state: &State<'_, AppState>,
    path: PathBuf,
    is_idle: bool,
) -> Result<String, String> {
    if image::open(&path).is_err() {
        return Err("Unsupported image format.".to_string());
    }
    let path_str = path.to_string_lossy().to_string();
    state.settings.update(|s| {
        if is_idle {
            s.idle_image_path = Some(path_str.clone());
        } else {
            s.talking_image_path = Some(path_str.clone());
        }
    });
    broadcast_settings(app, &state.settings);
    Ok(path_str)
}

/// FR-001: opens a native file picker restricted to PNG, validates it
/// decodes, and persists the path. Returns the chosen path (or an error
/// message matching the Error Handling table in SRS A.4).
#[tauri::command]
pub async fn select_idle_image(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    select_image(&app, &state, true).await
}

#[tauri::command]
pub async fn select_talking_image(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    select_image(&app, &state, false).await
}

async fn select_image(app: &AppHandle, state: &State<'_, AppState>, is_idle: bool) -> Result<String, String> {
    // IMPORTANT FIX: blocking_pick_file() blocks the calling thread while
    // waiting for the user to interact with the native dialog. Inside an
    // `async` command, that stalls Tauri's async runtime instead of
    // yielding properly, so the result never reliably makes it back to the
    // frontend's invoke() promise. The non-blocking pick_file() + oneshot
    // channel below awaits correctly without blocking any thread.
    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .add_filter("PNG Image", &["png"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });

    let file = rx.await.map_err(|_| "Dialog was closed unexpectedly.".to_string())?;

    let path = match file {
        Some(p) => p.into_path().map_err(|_| "Unsupported image format.".to_string())?,
        None => return Err("No file selected.".to_string()),
    };

    validate_and_store_image(app, state, path, is_idle)
}

/// v1.2 Phase 1: drag-and-drop image loading. The frontend already knows
/// the absolute path (from Tauri's native onDragDropEvent, which hands over
/// real filesystem paths — unlike a browser drop, which only gives opaque
/// File blobs) so this skips the dialog entirely and reuses the same
/// validate_and_store_image() path as the Browse buttons.
#[tauri::command]
pub fn set_image_from_dropped_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    is_idle: bool,
) -> Result<String, String> {
    validate_and_store_image(&app, &state, PathBuf::from(path), is_idle)
}

/// FR-002: enumerates input devices for the Control Window dropdown.
#[tauri::command]
pub fn list_microphones() -> Vec<MicrophoneOption> {
    AudioEngine::list_input_devices()
        .into_iter()
        .map(|(id, name)| MicrophoneOption { id, name })
        .collect()
}

#[tauri::command]
pub fn set_microphone(app: AppHandle, state: State<'_, AppState>, id: String) {
    state.settings.update(|s| s.microphone_device_id = Some(id.clone()));
    state.audio.stop();
    state.audio.start(Some(id));
    broadcast_settings(&app, &state.settings);
}

/// FR-003: live sensitivity update, no restart required.
#[tauri::command]
pub fn set_sensitivity(app: AppHandle, state: State<'_, AppState>, value: u8) {
    state.audio.set_threshold(value);
    state.settings.update(|s| s.sensitivity_threshold = value);
    broadcast_settings(&app, &state.settings);
}

/// v1.2 Phase 1: noise gate threshold (volume below this is treated as
/// silence before it ever reaches the sensitivity comparison).
#[tauri::command]
pub fn set_noise_gate(app: AppHandle, state: State<'_, AppState>, value: u8) {
    state.audio.set_noise_gate(value);
    state.settings.update(|s| s.noise_gate_threshold = value);
    broadcast_settings(&app, &state.settings);
}

/// v1.2 Phase 1: adjustable mouth hold time (was a hardcoded 200ms constant).
#[tauri::command]
pub fn set_hold_time(app: AppHandle, state: State<'_, AppState>, value: u32) {
    state.audio.set_hold_time_ms(value);
    state.settings.update(|s| s.mouth_hold_time_ms = value);
    broadcast_settings(&app, &state.settings);
}

/// FR-004: shows the Character Window. Blocked by the UI layer if idle/
/// talking images or a microphone aren't set yet (SRS A.4 error handling).
///
/// FIX: also re-broadcasts the current settings snapshot right after
/// showing the window. The Character Window loads hidden at startup, and
/// depending on exact timing it can finish attaching its event listener
/// slightly after an earlier settings-updated broadcast — meaning a newly
/// picked image could be missed until something else re-synced it (e.g. an
/// app restart). Pushing a fresh snapshot at the moment the window becomes
/// visible makes this deterministic instead of timing-dependent.
#[tauri::command]
pub fn launch_character(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.windows.launch()?;
    broadcast_settings(&app, &state.settings);
    Ok(())
}

#[tauri::command]
pub fn hide_character(state: State<'_, AppState>) -> Result<(), String> {
    state.windows.hide()
}

#[tauri::command]
pub fn set_always_on_top(state: State<'_, AppState>, value: bool) -> Result<(), String> {
    state.windows.set_always_on_top(value)
}

// --- v1.2 Phase 1: character window effects/behavior ---

/// Opacity is applied as CSS opacity on the sprite in the Character Window
/// (see events.rs doc comment on CharacterWindowState.opacity for why this
/// isn't an OS-level window property).
#[tauri::command]
pub fn set_character_opacity(app: AppHandle, state: State<'_, AppState>, value: f32) {
    state.settings.update(|s| s.character_window.opacity = value.clamp(0.0, 1.0));
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn set_locked(state: State<'_, AppState>, value: bool) -> Result<(), String> {
    state.windows.set_locked(value)
}

#[tauri::command]
pub fn set_click_through(state: State<'_, AppState>, value: bool) -> Result<(), String> {
    state.windows.set_click_through(value)
}

#[tauri::command]
pub fn set_flipped(app: AppHandle, state: State<'_, AppState>, value: bool) {
    state.settings.update(|s| s.character_window.flipped = value);
    broadcast_settings(&app, &state.settings);
}

/// v1.2 "character scaling hotkeys" — grows/shrinks the Character Window by
/// a fixed step. Geometry persistence happens automatically via the
/// existing resize-event listener in window_manager.rs.
#[tauri::command]
pub fn nudge_character_size(state: State<'_, AppState>, grow: bool) -> Result<(), String> {
    state.windows.nudge_size(grow)
}

// --- v1.2 Phase 2: character presentation ---

#[tauri::command]
pub fn set_rotation(app: AppHandle, state: State<'_, AppState>, value: f32) {
    state.settings.update(|s| s.character_window.rotation_deg = value.clamp(-180.0, 180.0));
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn set_shadow_enabled(app: AppHandle, state: State<'_, AppState>, value: bool) {
    state.settings.update(|s| s.character_window.shadow_enabled = value);
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn set_outline_enabled(app: AppHandle, state: State<'_, AppState>, value: bool) {
    state.settings.update(|s| s.character_window.outline_enabled = value);
    broadcast_settings(&app, &state.settings);
}

// --- v1.2 Phase 4: profiles ---

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> ProfilesUpdatedEvent {
    let (profiles, active_profile) = state.settings.list_profiles();
    ProfilesUpdatedEvent { profiles, active_profile }
}

/// Creates a new profile (starting from defaults) and switches to it.
/// Re-applies the mic/threshold for the new (default) settings, same as
/// switch_profile, since a brand-new profile has its own blank audio config.
#[tauri::command]
pub fn create_profile(app: AppHandle, state: State<'_, AppState>, name: String) {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    state.settings.create_profile(trimmed);
    apply_active_profile_audio(&state);
    broadcast_profiles(&app, &state.settings);
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn switch_profile(app: AppHandle, state: State<'_, AppState>, name: String) -> bool {
    let switched = state.settings.switch_profile(name);
    if switched {
        apply_active_profile_audio(&state);
        broadcast_profiles(&app, &state.settings);
        broadcast_settings(&app, &state.settings);
    }
    switched
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, state: State<'_, AppState>, name: String) {
    state.settings.delete_profile(name);
    apply_active_profile_audio(&state);
    broadcast_profiles(&app, &state.settings);
    broadcast_settings(&app, &state.settings);
}

/// After switching/creating/deleting a profile, the newly-active profile
/// may reference a different microphone/threshold/noise-gate/hold-time than
/// what's currently running — re-apply all of them so the audio engine
/// matches the profile that's now active, not the one that was active a
/// moment ago.
fn apply_active_profile_audio(state: &State<'_, AppState>) {
    let settings = state.settings.get();
    state.audio.stop();
    state.audio.start(settings.microphone_device_id.clone());
    state.audio.set_threshold(settings.sensitivity_threshold);
    state.audio.set_noise_gate(settings.noise_gate_threshold);
    state.audio.set_hold_time_ms(settings.mouth_hold_time_ms);
}

/// Returns the full current settings snapshot — used by src/main.js on
/// initial load to populate the UI (see boot sequence, Architecture C.4).
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsUpdatedEvent {
    (&state.settings.get()).into()
}
