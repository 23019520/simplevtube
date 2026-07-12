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
use crate::events::{
    Emote, EmoteRepositionModeEvent, EmoteTriggeredEvent, ProfilesUpdatedEvent, SettingsUpdatedEvent,
    EVT_EMOTE_REPOSITION_MODE, EVT_EMOTE_TRIGGERED, EVT_PROFILES_UPDATED, EVT_SETTINGS_UPDATED,
};
use crate::settings_manager::SettingsManager;
use crate::window_manager::WindowManager;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
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

/// v1.12 FIX: the crop editor saves every exported frame as a new file
/// under app-data/frames/ (see save_processed_frame) — nothing ever
/// deleted the OLD file when a frame was replaced or removed, so repeated
/// editing silently accumulated orphaned files forever. This checks that a
/// given path is genuinely inside our own managed frames/ folder (never
/// touches the user's original source images, wherever they live) before
/// deleting it. Best-effort: a failed delete is not surfaced as an error
/// to the user, since the settings mutation itself already succeeded and
/// a stray leftover file is a minor cleanup miss, not a functional bug.
fn cleanup_managed_frame_file(app: &AppHandle, path: &str) {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        return;
    };
    let frames_dir = app_data_dir.join("frames");
    let (Ok(canon_path), Ok(canon_frames_dir)) = (std::fs::canonicalize(path), std::fs::canonicalize(&frames_dir))
    else {
        return; // path doesn't exist or frames dir doesn't exist yet — nothing to clean up
    };
    if canon_path.starts_with(&canon_frames_dir) {
        let _ = std::fs::remove_file(&canon_path);
    }
}

/// Called once from main.rs's setup() to fire the initial settings-updated
/// and profiles-updated events (boot sequence step 2->3 in Architecture C.4).
pub fn broadcast_initial_settings(app: &AppHandle, settings: &SettingsManager) {
    broadcast_settings(app, settings);
    broadcast_profiles(app, settings);
}

/// v1.4: opens a native file picker restricted to PNG and validates it
/// decodes, but does NOT store it anywhere. The frontend's crop/position
/// editor (see main.js) opens on the returned raw path; only the EDITED
/// result (via save_processed_frame below) ever becomes an actual frame.
/// This one command replaces what used to be three separate dialog
/// implementations (idle, talking, emote) — the picking step is identical
/// regardless of what the picked image is eventually used for.
#[tauri::command]
pub async fn pick_image_file(app: AppHandle) -> Result<String, String> {
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
    if image::open(&path).is_err() {
        return Err("Unsupported image format.".to_string());
    }
    Ok(path.to_string_lossy().to_string())
}

/// v1.4: validates a path handed over directly (e.g. from drag-and-drop,
/// which already has the absolute path via Tauri's native drag-drop bridge
/// and doesn't need a dialog) before it's opened in the crop editor.
#[tauri::command]
pub fn validate_image_path(path: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    if image::open(&path_buf).is_err() {
        return Err("Unsupported image format.".to_string());
    }
    Ok(path)
}

/// v1.4 FIX: the crop editor needs pixel-level canvas access (toDataURL)
/// to export the cropped result, but images loaded via convertFileSrc's
/// asset:// protocol taint the canvas — the webview doesn't send
/// permissive CORS headers for that custom protocol, so the browser
/// treats it as cross-origin and blocks reading pixel data back out.
/// data: URLs are always same-origin/never taint a canvas, so the editor
/// loads images through this command instead of convertFileSrc.
#[tauri::command]
pub fn read_image_as_data_url(path: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(&path).map_err(|e| format!("Could not read image: {e}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{encoded}"))
}

/// v1.4: saves the crop editor's exported PNG (base64-encoded canvas data,
/// already cropped/positioned/resized to the standard frame dimensions in
/// the frontend) to a dedicated frames folder in app data. All actual
/// image manipulation happens in JS via Canvas — this command's only job
/// is decoding the base64 payload and writing bytes to disk, so there's no
/// duplicate cropping/scaling logic to keep in sync between Rust and JS.
#[tauri::command]
pub fn save_processed_frame(app: AppHandle, base64_png: String) -> Result<String, String> {
    use base64::Engine;

    // Frontend sends a raw base64 payload (prefix already stripped in JS),
    // but strip defensively in case a data: URL slips through.
    let raw = base64_png
        .split(',')
        .last()
        .unwrap_or(&base64_png);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| format!("Could not decode edited image: {e}"))?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve app data directory: {e}"))?;
    let frames_dir = app_data_dir.join("frames");
    std::fs::create_dir_all(&frames_dir).map_err(|e| format!("Could not create frames folder: {e}"))?;

    let filename = format!(
        "frame_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let out_path = frames_dir.join(filename);
    std::fs::write(&out_path, bytes).map_err(|e| format!("Could not save edited image: {e}"))?;

    Ok(out_path.to_string_lossy().to_string())
}

/// v1.3/v1.4: appends an already-picked-and-edited frame path. No dialog,
/// no validation — the crop editor guarantees the exported PNG is valid
/// and correctly sized, so this command is intentionally trivial.
#[tauri::command]
pub fn append_idle_frame(app: AppHandle, state: State<'_, AppState>, path: String) {
    state.settings.add_idle_frame(path);
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn append_talking_frame(app: AppHandle, state: State<'_, AppState>, path: String) {
    state.settings.add_talking_frame(path);
    broadcast_settings(&app, &state.settings);
}

/// v1.4: replaces a frame in place at a given index — used when re-editing
/// an existing thumbnail's crop/position rather than adding a new one.
/// v1.12: also deletes the OLD frame file from disk (if it's one of ours).
#[tauri::command]
pub fn replace_idle_frame(app: AppHandle, state: State<'_, AppState>, index: usize, path: String) {
    let old_path = state.settings.get().idle_frames.get(index).cloned();
    state.settings.replace_idle_frame(index, path);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn replace_talking_frame(app: AppHandle, state: State<'_, AppState>, index: usize, path: String) {
    let old_path = state.settings.get().talking_frames.get(index).cloned();
    state.settings.replace_talking_frame(index, path);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

/// v1.3: removes one frame from the idle/talking frame list by index.
/// v1.12: also deletes the removed frame's file from disk (if it's one of ours).
#[tauri::command]
pub fn remove_idle_frame(app: AppHandle, state: State<'_, AppState>, index: usize) {
    let old_path = state.settings.get().idle_frames.get(index).cloned();
    state.settings.remove_idle_frame(index);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn remove_talking_frame(app: AppHandle, state: State<'_, AppState>, index: usize) {
    let old_path = state.settings.get().talking_frames.get(index).cloned();
    state.settings.remove_talking_frame(index);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

/// v1.3: shared cycle speed for both idle_frames and talking_frames.
#[tauri::command]
pub fn set_frame_interval(app: AppHandle, state: State<'_, AppState>, value: u32) {
    state.settings.set_frame_interval(value);
    broadcast_settings(&app, &state.settings);
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

/// v1.12: Automatic Gain Control toggle — see audio_engine.rs for the
/// actual peak-follower implementation.
#[tauri::command]
pub fn set_agc_enabled(app: AppHandle, state: State<'_, AppState>, value: bool) {
    state.audio.set_agc_enabled(value);
    state.settings.update(|s| s.agc_enabled = value);
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

/// Keyboard-driven alternative to dragging: Ctrl+Arrow keys move the
/// character by a fixed step. Geometry persistence happens automatically
/// via the existing move-event listener in window_manager.rs.
#[tauri::command]
pub fn nudge_character_position(state: State<'_, AppState>, dx: f64, dy: f64) -> Result<(), String> {
    state.windows.nudge_position(dx, dy)
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

// --- v1.10: spring-physics reactive jiggle ---

#[tauri::command]
pub fn set_physics_enabled(app: AppHandle, state: State<'_, AppState>, value: bool) {
    state.settings.update(|s| s.character_window.physics_enabled = value);
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn set_physics_intensity(app: AppHandle, state: State<'_, AppState>, value: f32) {
    state.settings.update(|s| s.character_window.physics_intensity = value.clamp(0.0, 100.0));
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
    state.audio.set_agc_enabled(settings.agc_enabled);
}

// --- v1.3: pop-up emotes ---

/// Creates a blank emote (default name, no frames, 1500ms duration) and
/// returns it so the frontend can immediately render its card. Also
/// broadcasts the updated settings so every window's emote list stays synced.
#[tauri::command]
pub fn add_emote(app: AppHandle, state: State<'_, AppState>) -> Emote {
    let emote = state.settings.add_emote();
    broadcast_settings(&app, &state.settings);
    emote
}

/// v1.12: also deletes ALL of the emote's frame files from disk (if ours).
#[tauri::command]
pub fn delete_emote(app: AppHandle, state: State<'_, AppState>, id: String) {
    let old_frames = state.settings.find_emote(&id).map(|e| e.frame_paths).unwrap_or_default();
    state.settings.delete_emote(&id);
    for path in old_frames {
        cleanup_managed_frame_file(&app, &path);
    }
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn rename_emote(app: AppHandle, state: State<'_, AppState>, id: String, name: String) {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    state.settings.rename_emote(&id, trimmed);
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn set_emote_duration(app: AppHandle, state: State<'_, AppState>, id: String, duration_ms: u32) {
    state.settings.set_emote_duration(&id, duration_ms.clamp(200, 10_000));
    broadcast_settings(&app, &state.settings);
}

/// hotkey_digit of 0 (or omitted) clears the hotkey — Option<u8> doesn't
/// round-trip cleanly through every JS falsy-check path, so the frontend
/// sends None explicitly when clearing.
#[tauri::command]
pub fn set_emote_hotkey(app: AppHandle, state: State<'_, AppState>, id: String, hotkey_digit: Option<u8>) {
    state.settings.set_emote_hotkey(&id, hotkey_digit);
    broadcast_settings(&app, &state.settings);
}

/// v1.3/v1.4: appends an already-picked-and-edited frame to a specific
/// emote. No dialog, no validation — same reasoning as append_idle_frame.
#[tauri::command]
pub fn append_emote_frame(app: AppHandle, state: State<'_, AppState>, id: String, path: String) {
    state.settings.add_emote_frame(&id, path);
    broadcast_settings(&app, &state.settings);
}

/// v1.4: replaces an emote frame in place (re-editing an existing thumbnail).
/// v1.12: also deletes the OLD frame file from disk (if it's one of ours).
#[tauri::command]
pub fn replace_emote_frame(app: AppHandle, state: State<'_, AppState>, id: String, index: usize, path: String) {
    let old_path = state
        .settings
        .find_emote(&id)
        .and_then(|e| e.frame_paths.get(index).cloned());
    state.settings.replace_emote_frame(&id, index, path);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

#[tauri::command]
pub fn remove_emote_frame(app: AppHandle, state: State<'_, AppState>, id: String, index: usize) {
    let old_path = state
        .settings
        .find_emote(&id)
        .and_then(|e| e.frame_paths.get(index).cloned());
    state.settings.remove_emote_frame(&id, index);
    if let Some(old_path) = old_path {
        cleanup_managed_frame_file(&app, &old_path);
    }
    broadcast_settings(&app, &state.settings);
}

/// Shared logic: given an already-looked-up Emote, fires it. Used by both
/// the JS-invoked trigger_emote command (by id) and the global-hotkey path
/// (by assigned digit) in main.rs — one place decides "what does firing an
/// emote actually do."
fn fire_emote(app: &AppHandle, emote: &Emote) -> Result<(), String> {
    if emote.frame_paths.is_empty() {
        return Err("This emote has no images yet.".to_string());
    }
    let _ = app.emit(
        EVT_EMOTE_TRIGGERED,
        EmoteTriggeredEvent {
            frame_paths: emote.frame_paths.clone(),
            duration_ms: emote.duration_ms,
        },
    );
    Ok(())
}

/// Fires the emote. The Emote Window owns all playback timing itself once
/// it receives this event — this command's only job is "look up the emote,
/// hand its frames + duration to the event bus." No frame-cycling logic
/// lives here (see events.rs's EmoteTriggeredEvent doc comment).
#[tauri::command]
pub fn trigger_emote(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let emote = state
        .settings
        .find_emote(&id)
        .ok_or("That emote no longer exists.".to_string())?;
    fire_emote(&app, &emote)
}

/// v1.5: global hotkey path (Alt+digit) — NOT a #[tauri::command], since
/// nothing in JS calls this directly. Called from the global-shortcut
/// handler registered in main.rs, which fires system-wide (even while
/// another app has focus) rather than only while the Control Window does.
pub fn trigger_emote_by_hotkey(app: &AppHandle, digit: u8) {
    if let Some(state) = app.try_state::<AppState>() {
        let emote = state
            .settings
            .get()
            .emotes
            .into_iter()
            .find(|e| e.hotkey_digit == Some(digit));
        if let Some(emote) = emote {
            let _ = fire_emote(app, &emote);
        }
    }
}

/// v1.5: global hotkey path for character resize (Ctrl+=/Ctrl+-).
pub fn nudge_size_via_hotkey(app: &AppHandle, grow: bool) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.windows.nudge_size(grow);
    }
}

/// v1.5: global hotkey path for character movement (Ctrl+Arrow keys).
pub fn nudge_position_via_hotkey(app: &AppHandle, dx: f64, dy: f64) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.windows.nudge_position(dx, dy);
    }
}

/// v1.4 FIX: re-applies click-through for the Emote Window. The one-time
/// call during Rust's setup() can race with the window actually being
/// fully realized by the OS (same class of timing issue as the app-state
/// race fixed in main.rs) — calling this from the Emote Window's own JS on
/// load guarantees the window definitely exists by then, since its script
/// couldn't be running otherwise. Uses reapply_emote_click_through (not
/// setup_emote_window) so it doesn't re-attach geometry listeners a second
/// time on top of the one already attached at boot.
#[tauri::command]
pub fn finalize_emote_window(state: State<'_, AppState>) {
    state.windows.reapply_emote_click_through();
}

/// v1.6: toggles "reposition mode" — while enabled, the Emote Window
/// becomes draggable/resizable instead of click-through, and its own JS
/// shows a placeholder box so there's something visible to grab. Position
/// and size persist automatically via the same geometry-listener pattern
/// used for the Character Window.
#[tauri::command]
pub fn set_emote_reposition_mode(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    // Reposition mode being ON means the user needs to be able to click on
    // it, i.e. click-through must be OFF — hence the negation.
    state.windows.set_emote_click_through(!enabled)?;
    let _ = app.emit(EVT_EMOTE_REPOSITION_MODE, EmoteRepositionModeEvent { enabled });
    Ok(())
}

// --- v1.9: undo/redo ---

#[tauri::command]
pub fn undo_settings(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.settings.undo()?;
    apply_active_profile_audio(&state); // undo can switch profiles, which may change the active mic/thresholds
    broadcast_settings(&app, &state.settings);
    broadcast_profiles(&app, &state.settings);
    Ok(())
}

#[tauri::command]
pub fn redo_settings(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.settings.redo()?;
    apply_active_profile_audio(&state);
    broadcast_settings(&app, &state.settings);
    broadcast_profiles(&app, &state.settings);
    Ok(())
}

/// Returns the full current settings snapshot — used by src/main.js on
/// initial load to populate the UI (see boot sequence, Architecture C.4).
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsUpdatedEvent {
    (&state.settings.get()).into()
}
