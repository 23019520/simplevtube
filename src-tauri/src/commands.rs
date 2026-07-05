// commands.rs
//
// Every #[tauri::command] here corresponds exactly to a row in the
// architecture doc's C.3 "File-to-File Contracts" table. src/main.js calls
// these via invoke(); nothing else calls these directly.

use crate::audio_engine::AudioEngine;
use crate::events::{SettingsUpdatedEvent, EVT_SETTINGS_UPDATED};
use crate::settings_manager::SettingsManager;
use crate::window_manager::WindowManager;
use serde::Serialize;
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

/// Called once from main.rs's setup() to fire the initial settings-updated
/// event (boot sequence step 2->3 in Architecture doc C.4).
pub fn broadcast_initial_settings(app: &AppHandle, settings: &SettingsManager) {
    broadcast_settings(app, settings);
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

    // Basic validation: confirm the file actually decodes as a PNG,
    // not just that the extension matches (SRS FR-001.3).
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

/// Returns the full current settings snapshot — used by src/main.js on
/// initial load to populate the UI (see boot sequence, Architecture C.4).
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsUpdatedEvent {
    (&state.settings.get()).into()
}
