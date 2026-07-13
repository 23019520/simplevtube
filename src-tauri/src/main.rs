// Suppress the Windows console window in release builds.
// In dev mode (npm run dev / debug_assertions = true), the terminal stays
// visible so log output and Rust errors are readable. In release builds
// (npm run build), it's hidden — app behaves like a normal desktop app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// main.rs
//
// Boot sequence follows Architecture doc Part C.4 exactly:
//   1. Create windows, init event bus (implicit via AppHandle/Emitter)
//   2. settings_manager loads config (or defaults) -> broadcasts settings-updated
//   3. (src/main.js receives it and populates UI — frontend responsibility)
//   4. audio_engine opens the persisted mic, or falls back to default
//   5. window_manager restores Character Window geometry
//   6. Control Window becomes visible
//
// v1.5: adds SYSTEM-WIDE hotkeys via tauri-plugin-global-shortcut — the
// local DOM keydown handlers in main.js only ever fired while the Control
// Window had focus, which is useless mid-stream when OBS or a game has
// focus instead. These fire regardless of which app is focused.
//
// HONEST TRADEOFF, worth knowing: Ctrl+=/Ctrl+-/Ctrl+Arrow are common
// shortcuts in OTHER apps too (browser zoom, text navigation, some games).
// Registering them globally means SimpleVTube "steals" them system-wide
// while it's running. There's no per-shortcut opt-out in this version —
// if one of these conflicts with something you use elsewhere, the tradeoff
// right now is all-or-nothing for that key combo.

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
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use window_manager::WindowManager;

fn main() {
    // Fixed shortcuts, built once. Cloned into the handler closure below;
    // the originals are moved into setup() later for actual registration.
    let resize_grow = Shortcut::new(Some(Modifiers::CONTROL), Code::Equal);
    let resize_shrink = Shortcut::new(Some(Modifiers::CONTROL), Code::Minus);
    let move_up = Shortcut::new(Some(Modifiers::CONTROL), Code::ArrowUp);
    let move_down = Shortcut::new(Some(Modifiers::CONTROL), Code::ArrowDown);
    let move_left = Shortcut::new(Some(Modifiers::CONTROL), Code::ArrowLeft);
    let move_right = Shortcut::new(Some(Modifiers::CONTROL), Code::ArrowRight);

    // Alt+1 through Alt+9 are always registered as a fixed set (there are
    // only 9 possible digits) — which emote (if any) actually fires for a
    // given digit is looked up dynamically at trigger-time in
    // trigger_emote_by_hotkey, so adding/removing/reassigning emote
    // hotkeys never needs to re-register anything at the OS level.
    let emote_digit_codes: [(u8, Code); 9] = [
        (1, Code::Digit1),
        (2, Code::Digit2),
        (3, Code::Digit3),
        (4, Code::Digit4),
        (5, Code::Digit5),
        (6, Code::Digit6),
        (7, Code::Digit7),
        (8, Code::Digit8),
        (9, Code::Digit9),
    ];
    let emote_shortcuts: Vec<(u8, Shortcut)> = emote_digit_codes
        .iter()
        .map(|(d, c)| (*d, Shortcut::new(Some(Modifiers::ALT), *c)))
        .collect();

    let h_resize_grow = resize_grow.clone();
    let h_resize_shrink = resize_shrink.clone();
    let h_move_up = move_up.clone();
    let h_move_down = move_down.clone();
    let h_move_left = move_left.clone();
    let h_move_right = move_right.clone();
    let h_emote_shortcuts = emote_shortcuts.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return; // only act on key-down, not key-up
                    }
                    if shortcut == &h_resize_grow {
                        commands::nudge_size_via_hotkey(app, true);
                    } else if shortcut == &h_resize_shrink {
                        commands::nudge_size_via_hotkey(app, false);
                    } else if shortcut == &h_move_up {
                        commands::nudge_position_via_hotkey(app, 0.0, -10.0);
                    } else if shortcut == &h_move_down {
                        commands::nudge_position_via_hotkey(app, 0.0, 10.0);
                    } else if shortcut == &h_move_left {
                        commands::nudge_position_via_hotkey(app, -10.0, 0.0);
                    } else if shortcut == &h_move_right {
                        commands::nudge_position_via_hotkey(app, 10.0, 0.0);
                    } else {
                        for (digit, sc) in &h_emote_shortcuts {
                            if shortcut == sc {
                                commands::trigger_emote_by_hotkey(app, *digit);
                                break;
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Step 2: load settings (falls back to defaults / recovers from
            // corruption internally, per settings_manager.rs).
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            let settings = Arc::new(SettingsManager::load(app_data_dir));

            let initial_settings = settings.get();
            let audio = Arc::new(AudioEngine::new(
                app_handle.clone(),
                initial_settings.sensitivity_threshold,
                initial_settings.noise_gate_threshold,
                initial_settings.mouth_hold_time_ms,
                initial_settings.agc_enabled,
            ));

            let windows = Arc::new(WindowManager::new(app_handle.clone(), settings.clone()));

            // FIX: register state as early as physically possible — right
            // after constructing settings/audio/windows, before any of the
            // slower work below. Tauri's webviews start loading (and can
            // start calling invoke()) concurrently with this setup()
            // closure, not after it — so every command needs AppState to
            // exist as soon as it's ready, not after validation/geometry
            // work that has no reason to block it. This closes the
            // "state not managed" race that showed up once
            // validate_image_paths() had enough files to check that it
            // measurably widened the window.
            app.manage(AppState {
                settings: settings.clone(),
                audio: audio.clone(),
                windows: windows.clone(),
            });

            // Everything below can safely happen after state is managed.
            audio.start(initial_settings.microphone_device_id.clone());
            settings.validate_image_paths(); // FR-006.3: drop stale image paths
            windows.restore_geometry();
            windows.setup_emote_window();

            // Register the actual system-wide hotkeys. Failures here are
            // non-fatal (e.g. another app may have already grabbed one of
            // these combos) — the app should still run fine either way,
            // just without that one shortcut firing globally.
            let gs = app.global_shortcut();
            let _ = gs.register(resize_grow);
            let _ = gs.register(resize_shrink);
            let _ = gs.register(move_up);
            let _ = gs.register(move_down);
            let _ = gs.register(move_left);
            let _ = gs.register(move_right);
            for (_, sc) in emote_shortcuts {
                let _ = gs.register(sc);
            }

            // Broadcast initial settings snapshot so the Control Window can
            // populate itself as soon as it finishes loading (Step 3).
            commands::broadcast_initial_settings(&app_handle, &settings);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pick_image_file,
            commands::validate_image_path,
            commands::read_image_as_data_url,
            commands::save_processed_frame,
            commands::append_idle_frame,
            commands::append_talking_frame,
            commands::replace_idle_frame,
            commands::replace_talking_frame,
            commands::remove_idle_frame,
            commands::remove_talking_frame,
            commands::set_frame_interval,
            commands::list_microphones,
            commands::set_microphone,
            commands::set_sensitivity,
            commands::set_noise_gate,
            commands::set_hold_time,
            commands::set_agc_enabled,
            commands::launch_character,
            commands::hide_character,
            commands::set_always_on_top,
            commands::set_character_opacity,
            commands::set_locked,
            commands::set_click_through,
            commands::set_flipped,
            commands::nudge_character_size,
            commands::nudge_character_position,
            commands::set_rotation,
            commands::set_shadow_enabled,
            commands::set_outline_enabled,
            commands::set_physics_enabled,
            commands::set_physics_intensity,
            commands::list_profiles,
            commands::create_profile,
            commands::switch_profile,
            commands::delete_profile,
            commands::add_emote,
            commands::delete_emote,
            commands::rename_emote,
            commands::set_emote_duration,
            commands::set_emote_hotkey,
            commands::append_emote_frame,
            commands::replace_emote_frame,
            commands::remove_emote_frame,
            commands::trigger_emote,
            commands::finalize_emote_window,
            commands::set_emote_reposition_mode,
            commands::undo_settings,
            commands::redo_settings,
            commands::get_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SimpleVTube");
}            let settings = Arc::new(SettingsManager::load(app_data_dir));

            let initial_settings = settings.get();
            let audio = Arc::new(AudioEngine::new(
                app_handle.clone(),
                initial_settings.sensitivity_threshold,
                initial_settings.noise_gate_threshold,
                initial_settings.mouth_hold_time_ms,
                initial_settings.agc_enabled,
            ));

            let windows = Arc::new(WindowManager::new(app_handle.clone(), settings.clone()));

            // FIX: register state as early as physically possible — right
            // after constructing settings/audio/windows, before any of the
            // slower work below. Tauri's webviews start loading (and can
            // start calling invoke()) concurrently with this setup()
            // closure, not after it — so every command needs AppState to
            // exist as soon as it's ready, not after validation/geometry
            // work that has no reason to block it. This closes the
            // "state not managed" race that showed up once
            // validate_image_paths() had enough files to check that it
            // measurably widened the window.
            app.manage(AppState {
                settings: settings.clone(),
                audio: audio.clone(),
                windows: windows.clone(),
            });

            // Everything below can safely happen after state is managed.
            audio.start(initial_settings.microphone_device_id.clone());
            settings.validate_image_paths(); // FR-006.3: drop stale image paths
            windows.restore_geometry();
            windows.setup_emote_window();

            // Register the actual system-wide hotkeys. Failures here are
            // non-fatal (e.g. another app may have already grabbed one of
            // these combos) — the app should still run fine either way,
            // just without that one shortcut firing globally.
            let gs = app.global_shortcut();
            let _ = gs.register(resize_grow);
            let _ = gs.register(resize_shrink);
            let _ = gs.register(move_up);
            let _ = gs.register(move_down);
            let _ = gs.register(move_left);
            let _ = gs.register(move_right);
            for (_, sc) in emote_shortcuts {
                let _ = gs.register(sc);
            }

            // Broadcast initial settings snapshot so the Control Window can
            // populate itself as soon as it finishes loading (Step 3).
            commands::broadcast_initial_settings(&app_handle, &settings);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pick_image_file,
            commands::validate_image_path,
            commands::read_image_as_data_url,
            commands::save_processed_frame,
            commands::append_idle_frame,
            commands::append_talking_frame,
            commands::replace_idle_frame,
            commands::replace_talking_frame,
            commands::remove_idle_frame,
            commands::remove_talking_frame,
            commands::set_frame_interval,
            commands::list_microphones,
            commands::set_microphone,
            commands::set_sensitivity,
            commands::set_noise_gate,
            commands::set_hold_time,
            commands::set_agc_enabled,
            commands::launch_character,
            commands::hide_character,
            commands::set_always_on_top,
            commands::set_character_opacity,
            commands::set_locked,
            commands::set_click_through,
            commands::set_flipped,
            commands::nudge_character_size,
            commands::nudge_character_position,
            commands::set_rotation,
            commands::set_shadow_enabled,
            commands::set_outline_enabled,
            commands::set_physics_enabled,
            commands::set_physics_intensity,
            commands::list_profiles,
            commands::create_profile,
            commands::switch_profile,
            commands::delete_profile,
            commands::add_emote,
            commands::delete_emote,
            commands::rename_emote,
            commands::set_emote_duration,
            commands::set_emote_hotkey,
            commands::append_emote_frame,
            commands::replace_emote_frame,
            commands::remove_emote_frame,
            commands::trigger_emote,
            commands::finalize_emote_window,
            commands::set_emote_reposition_mode,
            commands::undo_settings,
            commands::redo_settings,
            commands::get_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SimpleVTube");
}
