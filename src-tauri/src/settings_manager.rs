// settings_manager.rs
//
// Owns config.json. Nothing else writes to disk for settings — audio_engine
// and window_manager call INTO this file, they never touch the file directly.
// This guarantees config.json is always a single consistent snapshot
// (Architecture doc, Part C.5).

use crate::events::{CharacterWindowState, SettingsUpdatedEvent};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub microphone_device_id: Option<String>,
    pub sensitivity_threshold: u8,
    pub idle_image_path: Option<String>,
    pub talking_image_path: Option<String>,
    pub character_window: CharacterWindowState,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            microphone_device_id: None,
            sensitivity_threshold: 35, // sensible default per FR-003
            idle_image_path: None,
            talking_image_path: None,
            character_window: CharacterWindowState::default(),
            theme: "dark".to_string(),
        }
    }
}

impl From<&Settings> for SettingsUpdatedEvent {
    fn from(s: &Settings) -> Self {
        SettingsUpdatedEvent {
            microphone_device_id: s.microphone_device_id.clone(),
            sensitivity_threshold: s.sensitivity_threshold,
            idle_image_path: s.idle_image_path.clone(),
            talking_image_path: s.talking_image_path.clone(),
            character_window: s.character_window.clone(),
            theme: s.theme.clone(),
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsManager {
    /// Loads config.json from the OS app-data directory, or falls back to
    /// defaults if missing/corrupt. A corrupt file is backed up (renamed)
    /// rather than silently overwritten, per SRS A.4/B.7 error strategy.
    pub fn load(app_data_dir: PathBuf) -> Self {
        if let Err(e) = fs::create_dir_all(&app_data_dir) {
            eprintln!("Could not create app-data dir: {e}");
        }
        let path = app_data_dir.join("config.json");

        let current = match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<Settings>(&contents) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!("config.json was corrupt ({e}), backing up and using defaults");
                    let backup_path = app_data_dir.join("config.corrupt.json");
                    let _ = fs::rename(&path, &backup_path);
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };

        let manager = Self {
            path,
            current: Mutex::new(current),
        };
        // Ensure a valid file exists on disk even on first run.
        manager.persist();
        manager
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    /// Generic mutate-then-persist helper. Every setter in commands.rs
    /// routes through here so writes are always centralized.
    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        {
            let mut guard = self.current.lock().unwrap();
            f(&mut guard);
        }
        self.persist();
        self.get()
    }

    /// Called by window_manager on resize/move-end (debounced there, not here).
    pub fn update_character_window(&self, state: CharacterWindowState) -> Settings {
        self.update(|s| s.character_window = state)
    }

    /// If a referenced image path no longer exists on disk, clear it so the
    /// UI re-prompts instead of the app crashing on a stale path (SRS FR-006.3).
    pub fn validate_image_paths(&self) -> Settings {
        self.update(|s| {
            if let Some(p) = &s.idle_image_path {
                if !PathBuf::from(p).exists() {
                    s.idle_image_path = None;
                }
            }
            if let Some(p) = &s.talking_image_path {
                if !PathBuf::from(p).exists() {
                    s.talking_image_path = None;
                }
            }
        })
    }

    fn persist(&self) {
        let settings = self.current.lock().unwrap();
        match serde_json::to_string_pretty(&*settings) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.path, json) {
                    eprintln!("Failed to write config.json: {e}");
                }
            }
            Err(e) => eprintln!("Failed to serialize settings: {e}"),
        }
    }
}
