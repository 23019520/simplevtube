// main.rs
//
// Boot sequence follows Architecture doc Part C.4 exactly:
//   1. Create windows, init event bus (implicit via AppHandle/Emitter)
//   2. settings_manager loads config (or defaults) -> broadcasts settings-updated
//   3. (src/main.js receives it and populates UI — frontend responsibility)
//   4. audio_engine opens the persisted mic, or falls back to default
//   5. window_manager restores Character Window geometry
//   6. Control Window becomes visible

mod audio_engine;
mod commands;
mod events;
mod settings_manager;
mod window_manager;

use audio_engine::AudioEngine;
use commands::AppState;
use settings_manager::SettingsManager;
use std::sync::Arc;
use tauri::Manager;
use window_manager::WindowManager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Step 2: load settings (falls back to defaults / recovers from
            // corruption internally, per settings_manager.rs).
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            let settings = Arc::new(SettingsManager::load(app_data_dir));
            settings.validate_image_paths(); // FR-006.3: drop stale image paths

            // Step 4: start audio engine on the persisted device (or default).
            let audio = Arc::new(AudioEngine::new(
                app_handle.clone(),
                settings.get().sensitivity_threshold,
            ));
            let mic_id = settings.get().microphone_device_id;
            audio.start(mic_id);

            // Step 5: restore Character Window geometry.
            let windows = Arc::new(WindowManager::new(app_handle.clone(), settings.clone()));
            windows.restore_geometry();

            app.manage(AppState {
                settings: settings.clone(),
                audio,
                windows,
            });

            // Broadcast initial settings snapshot so the Control Window can
            // populate itself as soon as it finishes loading (Step 3).
            commands::broadcast_initial_settings(&app_handle, &settings);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::select_idle_image,
            commands::select_talking_image,
            commands::list_microphones,
            commands::set_microphone,
            commands::set_sensitivity,
            commands::launch_character,
            commands::hide_character,
            commands::set_always_on_top,
            commands::get_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SimpleVTube");
}
