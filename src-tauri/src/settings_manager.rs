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

use crate::events::{CharacterWindowState, Emote, EmoteWindowState, SettingsUpdatedEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub microphone_device_id: Option<String>,
    pub sensitivity_threshold: u8,
    pub noise_gate_threshold: u8,
    pub mouth_hold_time_ms: u32,
    /// LEGACY (pre-v1.3): superseded by idle_frames/talking_frames below.
    /// Kept as-is (never changing an existing field's type — see the
    /// migration note on load()) so old configs still parse; migrate()
    /// folds these into the new frame lists at load time.
    pub idle_image_path: Option<String>,
    pub talking_image_path: Option<String>,
    /// v1.3: multi-frame cycling for idle/talking (see SettingsUpdatedEvent
    /// doc comment in events.rs for the full rationale).
    pub idle_frames: Vec<String>,
    pub talking_frames: Vec<String>,
    pub frame_interval_ms: u32,
    pub character_window: CharacterWindowState,
    pub theme: String,
    /// v1.3: pop-up emotes.
    pub emotes: Vec<Emote>,
    /// v1.6: where and at what size emotes pop up on screen.
    pub emote_window: EmoteWindowState,
}

// FIX: manual Default impl with sensible values (35, "dark", etc.) — NOT
// #[derive(Default)], which would silently give threshold=0, theme="", and
// every other field its type's zero-value instead. The container-level
// #[serde(default)] attribute above still works correctly with a manual
// impl: when an old config.json is missing a field (e.g. an older version
// that predates noiseGateThreshold), serde fills in that ONE missing field
// from this Default impl while keeping every field that IS present in the
// file — this is what fixed the "mic reset to nothing" bug.
impl Default for Settings {
    fn default() -> Self {
        Self {
            microphone_device_id: None,
            sensitivity_threshold: 35, // sensible default per FR-003
            noise_gate_threshold: 6,
            mouth_hold_time_ms: 200,
            idle_image_path: None,
            talking_image_path: None,
            idle_frames: Vec::new(),
            talking_frames: Vec::new(),
            frame_interval_ms: 150,
            character_window: CharacterWindowState::default(),
            theme: "dark".to_string(),
            emotes: Vec::new(),
            emote_window: EmoteWindowState::default(),
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
            idle_frames: s.idle_frames.clone(),
            talking_frames: s.talking_frames.clone(),
            frame_interval_ms: s.frame_interval_ms,
            character_window: s.character_window.clone(),
            theme: s.theme.clone(),
            emotes: s.emotes.clone(),
            emote_window: s.emote_window.clone(),
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

/// v1.3 migration: folds the legacy single-image fields into the new
/// frame-list fields for every profile, if the frame lists are still
/// empty. Idempotent and safe to run on every load — once idle_frames is
/// populated, this becomes a no-op for that profile forever after.
fn migrate_legacy_single_frame_fields(store: &mut ProfileStore) {
    for settings in store.profiles.values_mut() {
        if settings.idle_frames.is_empty() {
            if let Some(path) = &settings.idle_image_path {
                settings.idle_frames = vec![path.clone()];
            }
        }
        if settings.talking_frames.is_empty() {
            if let Some(path) = &settings.talking_image_path {
                settings.talking_frames = vec![path.clone()];
            }
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
    current: Mutex<ProfileStore>,
    // v1.9: undo/redo. Each entry is (profile name, that profile's Settings
    // snapshot BEFORE the change) so undo/redo work correctly even if you
    // switch profiles in between — undoing always targets the profile the
    // change actually happened on, switching back to it if needed.
    undo_stack: Mutex<Vec<(String, Settings)>>,
    redo_stack: Mutex<Vec<(String, Settings)>>,
    // Coalescing: without this, dragging a slider (which fires many rapid
    // updates) would push one undo step per pixel of movement, making undo
    // useless for that kind of change. Skipping snapshots within a short
    // window of the last one groups a whole drag/typing burst into a
    // single undo step instead.
    last_snapshot_at: Mutex<Option<Instant>>,
}

const UNDO_STACK_LIMIT: usize = 30;
const UNDO_COALESCE_WINDOW: Duration = Duration::from_millis(500);

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

        let mut store = store;
        migrate_legacy_single_frame_fields(&mut store);

        let manager = Self {
            path,
            current: Mutex::new(store),
            undo_stack: Mutex::new(Vec::new()),
            redo_stack: Mutex::new(Vec::new()),
            last_snapshot_at: Mutex::new(None),
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

    /// v1.9: records a pre-change snapshot for undo, unless one was already
    /// recorded within the last UNDO_COALESCE_WINDOW (groups rapid changes
    /// like a slider drag into a single undo step). Always clears the redo
    /// stack — making any new change invalidates whatever was undone
    /// before it, same as every standard undo/redo implementation.
    fn maybe_push_undo_snapshot(&self) {
        let mut last = self.last_snapshot_at.lock().unwrap();
        let should_push = last.map(|t| t.elapsed() >= UNDO_COALESCE_WINDOW).unwrap_or(true);
        if !should_push {
            return;
        }
        *last = Some(Instant::now());
        drop(last);

        let store = self.current.lock().unwrap();
        let active = store.active_profile.clone();
        if let Some(settings) = store.profiles.get(&active) {
            let mut undo = self.undo_stack.lock().unwrap();
            undo.push((active, settings.clone()));
            if undo.len() > UNDO_STACK_LIMIT {
                undo.remove(0);
            }
        }
        drop(store);
        self.redo_stack.lock().unwrap().clear();
    }

    /// Generic mutate-then-persist helper, applied to the active profile.
    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        self.maybe_push_undo_snapshot();
        {
            let mut store = self.current.lock().unwrap();
            let active = store.active_profile.clone();
            let settings = store.profiles.entry(active).or_insert_with(Settings::default);
            f(settings);
        }
        self.persist();
        self.get()
    }

    /// If a referenced image path no longer exists on disk, clear it so the
    /// UI re-prompts instead of the app crashing on a stale path (SRS FR-006.3).
    /// v1.3: also prunes any missing paths out of the frame lists and emotes.
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
            s.idle_frames.retain(|p| PathBuf::from(p).exists());
            s.talking_frames.retain(|p| PathBuf::from(p).exists());
            for emote in s.emotes.iter_mut() {
                emote.frame_paths.retain(|p| PathBuf::from(p).exists());
            }
        })
    }

    // --- v1.3: avatar multi-frame management ---

    pub fn add_idle_frame(&self, path: String) -> Settings {
        self.update(|s| s.idle_frames.push(path))
    }

    pub fn add_talking_frame(&self, path: String) -> Settings {
        self.update(|s| s.talking_frames.push(path))
    }

    pub fn remove_idle_frame(&self, index: usize) -> Settings {
        self.update(|s| {
            if index < s.idle_frames.len() {
                s.idle_frames.remove(index);
            }
        })
    }

    pub fn remove_talking_frame(&self, index: usize) -> Settings {
        self.update(|s| {
            if index < s.talking_frames.len() {
                s.talking_frames.remove(index);
            }
        })
    }

    /// Replaces an existing frame in place (used when re-editing a
    /// thumbnail's crop/position rather than adding a new frame).
    pub fn replace_idle_frame(&self, index: usize, path: String) -> Settings {
        self.update(|s| {
            if let Some(slot) = s.idle_frames.get_mut(index) {
                *slot = path;
            }
        })
    }

    pub fn replace_talking_frame(&self, index: usize, path: String) -> Settings {
        self.update(|s| {
            if let Some(slot) = s.talking_frames.get_mut(index) {
                *slot = path;
            }
        })
    }

    pub fn set_frame_interval(&self, value: u32) -> Settings {
        self.update(|s| s.frame_interval_ms = value)
    }

    // --- v1.3: emote management ---

    pub fn add_emote(&self) -> Emote {
        let id = format!(
            "emote_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let emote = Emote {
            id: id.clone(),
            ..Emote::default()
        };
        self.update(|s| s.emotes.push(emote.clone()));
        emote
    }

    pub fn delete_emote(&self, id: &str) -> Settings {
        self.update(|s| s.emotes.retain(|e| e.id != id))
    }

    pub fn rename_emote(&self, id: &str, name: String) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                e.name = name;
            }
        })
    }

    pub fn set_emote_duration(&self, id: &str, duration_ms: u32) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                e.duration_ms = duration_ms;
            }
        })
    }

    pub fn set_emote_hotkey(&self, id: &str, hotkey_digit: Option<u8>) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                e.hotkey_digit = hotkey_digit;
            }
        })
    }

    pub fn add_emote_frame(&self, id: &str, path: String) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                e.frame_paths.push(path);
            }
        })
    }

    pub fn remove_emote_frame(&self, id: &str, index: usize) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                if index < e.frame_paths.len() {
                    e.frame_paths.remove(index);
                }
            }
        })
    }

    pub fn replace_emote_frame(&self, id: &str, index: usize, path: String) -> Settings {
        self.update(|s| {
            if let Some(e) = s.emotes.iter_mut().find(|e| e.id == id) {
                if let Some(slot) = e.frame_paths.get_mut(index) {
                    *slot = path;
                }
            }
        })
    }

    pub fn find_emote(&self, id: &str) -> Option<Emote> {
        self.get().emotes.into_iter().find(|e| e.id == id)
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

    // --- v1.9: undo/redo ---

    /// Restores the most recent pre-change snapshot. If the change being
    /// undone happened on a different profile than the one currently
    /// active, switches to that profile too — so undo is always
    /// meaningful, never silently a no-op because you switched profiles
    /// since making the change.
    pub fn undo(&self) -> Result<Settings, String> {
        let (profile_name, snapshot) = {
            let mut stack = self.undo_stack.lock().unwrap();
            stack.pop().ok_or("Nothing to undo.".to_string())?
        };

        let current_for_redo = {
            let mut store = self.current.lock().unwrap();
            let current_for_redo = store.profiles.get(&profile_name).cloned().unwrap_or_default();
            store.profiles.insert(profile_name.clone(), snapshot);
            store.active_profile = profile_name.clone();
            current_for_redo
        };

        self.redo_stack.lock().unwrap().push((profile_name, current_for_redo));
        *self.last_snapshot_at.lock().unwrap() = Some(Instant::now()); // don't let the next real edit coalesce with this
        self.persist();
        Ok(self.get())
    }

    /// Symmetric opposite of undo(). Same cross-profile-switching behavior.
    pub fn redo(&self) -> Result<Settings, String> {
        let (profile_name, snapshot) = {
            let mut stack = self.redo_stack.lock().unwrap();
            stack.pop().ok_or("Nothing to redo.".to_string())?
        };

        let current_for_undo = {
            let mut store = self.current.lock().unwrap();
            let current_for_undo = store.profiles.get(&profile_name).cloned().unwrap_or_default();
            store.profiles.insert(profile_name.clone(), snapshot);
            store.active_profile = profile_name.clone();
            current_for_undo
        };

        self.undo_stack.lock().unwrap().push((profile_name, current_for_undo));
        *self.last_snapshot_at.lock().unwrap() = Some(Instant::now());
        self.persist();
        Ok(self.get())
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
