// settings_manager.rs
//
// Owns config.json. Nothing else writes to disk for settings — audio_engine
// and window_manager call INTO this file, they never touch the file directly.
// This guarantees config.json is always a single consistent snapshot
// (Architecture doc, Part C.5).
//
// v1.2 CHANGE: config.json now stores a ProfileStore (multiple named
// Settings, one active) instead of a single flat Settings object, to
// support the "multiple character profiles" feature. Existing v1/v1.1
// config files (a flat Settings object) are automatically migrated into a
// single "Default" profile on first load — nobody's existing setup breaks
// or resets just because this version added profiles.

use crate::events::{CharacterWindowState, SettingsUpdatedEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub microphone_device_id: Option<String>,
    pub sensitivity_threshold: u8,
    pub noise_gate_threshold: u8,
    pub mouth_hold_time_ms: u32,
    pub idle_image_path: Option<String>,
    pub talking_image_path: Option<String>,
    pub character_window: CharacterWindowState,
    pub theme: String,
}

impl Settings {
    fn defaults() -> Self {
        Self {
            microphone_device_id: None,
            sensitivity_threshold: 35, // sensible default per FR-003
            noise_gate_threshold: 6,
            mouth_hold_time_ms: 200,
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
            noise_gate_threshold: s.noise_gate_threshold,
            mouth_hold_time_ms: s.mouth_hold_time_ms,
            idle_image_path: s.idle_image_path.clone(),
            talking_image_path: s.talking_image_path.clone(),
            character_window: s.character_window.clone(),
            theme: s.theme.clone(),
        }
    }
}

/// v1.2: the on-disk root shape. BTreeMap keeps profile names in a stable
/// (alphabetical) order for listing in the UI, rather than random HashMap
/// iteration order.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStore {
    pub active_profile: String,
    pub profiles: BTreeMap<String, Settings>,
}

impl Default for ProfileStore {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert("Default".to_string(), Settings::default());
        Self {
            active_profile: "Default".to_string(),
            profiles,
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
    current: Mutex<ProfileStore>,
}

impl SettingsManager {
    /// Loads config.json from the OS app-data directory, or falls back to
    /// defaults if missing/corrupt. Handles three cases:
    ///   1. Already the new ProfileStore shape -> load directly.
    ///   2. The old v1/v1.1 flat Settings shape -> migrate into a "Default"
    ///      profile automatically, preserving the user's existing setup.
    ///   3. Neither parses -> back up the corrupt file and start fresh.
    pub fn load(app_data_dir: PathBuf) -> Self {
        if let Err(e) = fs::create_dir_all(&app_data_dir) {
            eprintln!("Could not create app-data dir: {e}");
        }
        let path = app_data_dir.join("config.json");

        let store = match fs::read_to_string(&path) {
            Ok(contents) => {
                if let Ok(store) = serde_json::from_str::<ProfileStore>(&contents) {
                    store
                } else if let Ok(legacy_settings) = serde_json::from_str::<Settings>(&contents) {
                    eprintln!("Migrating pre-v1.2 config into a \"Default\" profile.");
                    let mut profiles = BTreeMap::new();
                    profiles.insert("Default".to_string(), legacy_settings);
                    ProfileStore {
                        active_profile: "Default".to_string(),
                        profiles,
                    }
                } else {
                    eprintln!("config.json was corrupt, backing up and using defaults");
                    let backup_path = app_data_dir.join("config.corrupt.json");
                    let _ = fs::rename(&path, &backup_path);
                    ProfileStore::default()
                }
            }
            Err(_) => ProfileStore::default(),
        };

        let manager = Self {
            path,
            current: Mutex::new(store),
        };
        manager.persist();
        manager
    }

    /// Returns the ACTIVE profile's settings snapshot.
    pub fn get(&self) -> Settings {
        let store = self.current.lock().unwrap();
        store
            .profiles
            .get(&store.active_profile)
            .cloned()
            .unwrap_or_default()
    }

    /// Generic mutate-then-persist helper, applied to the active profile.
    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        {
            let mut store = self.current.lock().unwrap();
            let active = store.active_profile.clone();
            let settings = store.profiles.entry(active).or_insert_with(Settings::default);
            f(settings);
        }
        self.persist();
        self.get()
    }

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

    // --- v1.2 profile management (Phase 4 of the v2 roadmap) ---

    /// Returns (all profile names, currently active name).
    pub fn list_profiles(&self) -> (Vec<String>, String) {
        let store = self.current.lock().unwrap();
        (store.profiles.keys().cloned().collect(), store.active_profile.clone())
    }

    /// Creates a new profile with default settings and switches to it.
    /// If a profile with this name already exists, just switches to it
    /// instead of overwriting it.
    pub fn create_profile(&self, name: String) {
        {
            let mut store = self.current.lock().unwrap();
            store.profiles.entry(name.clone()).or_insert_with(Settings::default);
            store.active_profile = name;
        }
        self.persist();
    }

    /// Switches the active profile. Returns false if the name doesn't exist
    /// (the caller should treat this as a no-op, not create one implicitly).
    pub fn switch_profile(&self, name: String) -> bool {
        let mut store = self.current.lock().unwrap();
        if store.profiles.contains_key(&name) {
            store.active_profile = name;
            drop(store);
            self.persist();
            true
        } else {
            false
        }
    }

    /// Deletes a profile. Refuses to delete the last remaining profile so
    /// the app can never end up with zero profiles. If the active profile
    /// is deleted, falls back to whichever profile is alphabetically first.
    pub fn delete_profile(&self, name: String) {
        let mut store = self.current.lock().unwrap();
        if store.profiles.len() <= 1 {
            return;
        }
        store.profiles.remove(&name);
        if store.active_profile == name {
            store.active_profile = store
                .profiles
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "Default".to_string());
        }
        drop(store);
        self.persist();
    }

    fn persist(&self) {
        let store = self.current.lock().unwrap();
        match serde_json::to_string_pretty(&*store) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.path, json) {
                    eprintln!("Failed to write config.json: {e}");
                }
            }
            Err(e) => eprintln!("Failed to serialize settings: {e}"),
        }
    }
}