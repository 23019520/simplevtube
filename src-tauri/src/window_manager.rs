// window_manager.rs
//
// Owns the Character Window: showing/hiding it, always-on-top toggling, and
// persisting its geometry back through settings_manager on move/resize-end
// (SRS FR-004, Architecture B.5). The Control Window itself is just a normal
// Tauri window created from tauri.conf.json, so it doesn't need bespoke
// management here beyond being looked up by label.

use crate::settings_manager::SettingsManager;
use std::sync::Arc;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindow};

pub const CHARACTER_WINDOW_LABEL: &str = "character";
pub const EMOTE_WINDOW_LABEL: &str = "emote";

pub struct WindowManager {
    app: AppHandle,
    settings: Arc<SettingsManager>,
}

impl WindowManager {
    pub fn new(app: AppHandle, settings: Arc<SettingsManager>) -> Self {
        Self { app, settings }
    }

    fn character_window(&self) -> Option<WebviewWindow> {
        self.app.get_webview_window(CHARACTER_WINDOW_LABEL)
    }

    fn emote_window(&self) -> Option<WebviewWindow> {
        self.app.get_webview_window(EMOTE_WINDOW_LABEL)
    }

    /// v1.6: restores saved position/size if the user has customized it via
    /// reposition mode, else centers it (first-run default, unchanged from
    /// before this feature existed). Also applies click-through, and wires
    /// up geometry persistence. Called once at boot.
    pub fn setup_emote_window(&self) {
        if let Some(win) = self.emote_window() {
            let saved = self.settings.get().emote_window;
            let _ = win.set_size(LogicalSize::new(saved.width, saved.height));
            match (saved.x, saved.y) {
                (Some(x), Some(y)) => {
                    let _ = win.set_position(LogicalPosition::new(x, y));
                }
                _ => {
                    let _ = win.center();
                }
            }
            let _ = win.set_ignore_cursor_events(true);
            self.attach_emote_geometry_listeners();
        }
    }

    /// Re-applies just click-through, from a guaranteed-safe timing point
    /// (the Emote Window's own JS calling this on load — see
    /// finalize_emote_window in commands.rs). Deliberately doesn't touch
    /// position or re-attach listeners, since setup_emote_window already
    /// did that once at boot and doing it twice would double-write on
    /// every move/resize.
    pub fn reapply_emote_click_through(&self) {
        if let Some(win) = self.emote_window() {
            let _ = win.set_ignore_cursor_events(true);
        }
    }

    /// v1.6: toggles whether the Emote Window can be dragged/resized right
    /// now. true = reposition mode (interactive), false = normal operation
    /// (click-through, invisible until an emote fires).
    pub fn set_emote_click_through(&self, click_through: bool) -> Result<(), String> {
        let win = self
            .emote_window()
            .ok_or("Emote window not found")?;
        win.set_ignore_cursor_events(click_through).map_err(|e| e.to_string())
    }

    fn attach_emote_geometry_listeners(&self) {
        if let Some(win) = self.emote_window() {
            let settings = self.settings.clone();
            let win_for_closure = win.clone();
            win.on_window_event(move |event| match event {
                tauri::WindowEvent::Moved(pos) => {
                    let scale = win_for_closure.scale_factor().unwrap_or(1.0);
                    let logical = pos.to_logical::<f64>(scale);
                    settings.update(|s| {
                        s.emote_window.x = Some(logical.x);
                        s.emote_window.y = Some(logical.y);
                    });
                }
                tauri::WindowEvent::Resized(size) => {
                    let scale = win_for_closure.scale_factor().unwrap_or(1.0);
                    let logical = size.to_logical::<f64>(scale);
                    settings.update(|s| {
                        s.emote_window.width = logical.width.max(80.0);
                        s.emote_window.height = logical.height.max(80.0);
                    });
                }
                _ => {}
            });
        }
    }

    /// Restores the Character Window's last known geometry from settings.
    /// Called once at boot (main.rs step 5 in the architecture doc).
    pub fn restore_geometry(&self) {
        let saved = self.settings.get().character_window;
        if let Some(win) = self.character_window() {
            let _ = win.set_position(LogicalPosition::new(saved.x, saved.y));
            let _ = win.set_size(LogicalSize::new(saved.width, saved.height));
            let _ = win.set_always_on_top(saved.always_on_top);
            let _ = win.set_resizable(!saved.locked);
            let _ = win.set_ignore_cursor_events(saved.click_through);
            self.attach_geometry_listeners();
        }
    }

    /// Shows the Character Window (FR-004). Called by the "Launch Character"
    /// button via commands.rs.
    pub fn launch(&self) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn hide(&self) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.hide().map_err(|e| e.to_string())
    }

    pub fn set_always_on_top(&self, value: bool) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.set_always_on_top(value).map_err(|e| e.to_string())?;
        self.settings.update(|s| s.character_window.always_on_top = value);
        Ok(())
    }

    /// Wires move/resize events so geometry is persisted on change-end.
    /// Tauri's `on_window_event` fires per-event rather than only at
    /// drag-end, so we debounce naturally by just always writing the
    /// latest value — settings_manager's write is cheap and idempotent,
    /// and this still satisfies "persist on resize/move" from FR-006.
    fn attach_geometry_listeners(&self) {
        if let Some(win) = self.character_window() {
            let settings = self.settings.clone();
            let win_for_closure = win.clone();
            win.on_window_event(move |event| match event {
                tauri::WindowEvent::Moved(pos) => {
                    let scale = win_for_closure.scale_factor().unwrap_or(1.0);
                    let logical = pos.to_logical::<f64>(scale);
                    settings.update(|s| {
                        s.character_window.x = logical.x;
                        s.character_window.y = logical.y;
                    });
                }
                tauri::WindowEvent::Resized(size) => {
                    let scale = win_for_closure.scale_factor().unwrap_or(1.0);
                    let logical = size.to_logical::<f64>(scale);
                    settings.update(|s| {
                        s.character_window.width = logical.width;
                        s.character_window.height = logical.height;
                    });
                }
                _ => {}
            });
        }
    }

    // --- v1.2 additions (Phase 1 of the v2 roadmap) ---

    /// Mouse clicks pass through the Character Window entirely when true.
    /// Uses Tauri's set_ignore_cursor_events, a long-standing stable API
    /// (unlike window opacity, which has no reliable cross-platform
    /// equivalent — see events.rs's CharacterWindowState.opacity doc).
    pub fn set_click_through(&self, value: bool) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.set_ignore_cursor_events(value).map_err(|e| e.to_string())?;
        self.settings.update(|s| s.character_window.click_through = value);
        Ok(())
    }

    /// Disables OS-level resizing when locked. (Moving an already-borderless
    /// window isn't separately controllable via a stable Tauri API, so
    /// "lock" scopes to resize, which is the primary accidental-change risk
    /// during a live stream.)
    pub fn set_locked(&self, value: bool) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.set_resizable(!value).map_err(|e| e.to_string())?;
        self.settings.update(|s| s.character_window.locked = value);
        Ok(())
    }

    /// Grows or shrinks the Character Window by a fixed step, for the
    /// "character scaling hotkeys" feature. Reuses the exact same
    /// set_size() path as manual corner-drag resize, so the existing
    /// geometry-persistence listener (attach_geometry_listeners) picks up
    /// and saves the new size automatically — no separate persistence
    /// logic needed here.
    pub fn nudge_size(&self, grow: bool) -> Result<(), String> {
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        let current = self.settings.get().character_window;
        let factor: f64 = if grow { 1.08 } else { 1.0 / 1.08 };
        let new_width = (current.width * factor).clamp(80.0, 2400.0);
        let new_height = (current.height * factor).clamp(80.0, 2400.0);
        win.set_size(LogicalSize::new(new_width, new_height))
            .map_err(|e| e.to_string())
    }

    /// Keyboard-driven alternative to dragging (Ctrl+Arrow keys in the
    /// Control Window). Respects the lock flag, same as dragging does —
    /// "Lock position & size" should mean nothing moves it, full stop.
    pub fn nudge_position(&self, dx: f64, dy: f64) -> Result<(), String> {
        let current = self.settings.get().character_window;
        if current.locked {
            return Ok(()); // silently no-op rather than error; locked is an expected state, not a failure
        }
        let win = self
            .character_window()
            .ok_or("Character window not found")?;
        win.set_position(LogicalPosition::new(current.x + dx, current.y + dy))
            .map_err(|e| e.to_string())
    }
}
