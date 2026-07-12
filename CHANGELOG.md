# Changelog

All notable changes to SimpleVTube, in reverse chronological order.

## v1.12 — Final Sprint

### Fixed
- Orphaned frame files no longer accumulate forever. Every re-crop or removal of an idle/talking/emote frame now deletes the old exported file too — scoped strictly to the app's own managed `frames/` folder, never touching original source images.

### Added
- **Automatic Gain Control** — a slow peak-follower tracks how loud a microphone tends to get and applies a dynamic gain multiplier so quiet and loud mics both land in a similar usable range. Off by default so existing tuned setups aren't silently affected.
- Character and Emote windows magnetically snap to screen edges/center when dragged close.
- Numeric zoom percentage readout in the crop editor.
- **Stream Mode** — collapses the Control Window to just the essentials (profile, launch/hide, status) for a clean workspace while live; every setting stays reachable via the command palette.

## v1.11 — Idle Breathing

### Added
- Continuous idle breathing animation: a slow vertical sway with a subtle scale pulse, running independently of the reactive talking-bounce spring and blended additively at render time.

## v1.10 — Reactive Physics

### Added
- Real spring-damper physics simulation driving the character's bounce on speech (not CSS transitions) — a damped spring on vertical position with squash/stretch derived from velocity. Talking triggers an impulse "pop," and continued speech adds small kicks scaled to loudness. Toggleable with an intensity slider; costs nothing when disabled.

## v1.9 — Waveform & Undo/Redo

### Added
- Live scrolling audio waveform (last ~6 seconds of mic input) above the VU meter.
- Undo/Redo (Ctrl+Z / Ctrl+Shift+Z) across all settings changes, with time-based coalescing so rapid changes (e.g. a slider drag) become a single undo step rather than one per pixel of movement.

## v1.8 — Command Palette

### Added
- Fuzzy-search command palette (Ctrl+K) for instant access to every action: launch/hide the character, switch profiles, fire any emote, toggle any setting.

## v1.7 — Onion Skinning

### Added
- Onion-skin reference frames in the crop editor. Positioning a talking frame shows the idle frame ghosted beneath it (and vice versa) so pose stays aligned across the state swap, with same-state history as a secondary reference. Toggleable.

## v1.6 — Emote Positioning

### Added
- "Position & resize emote popup" reposition mode — a draggable/resizable placeholder box for setting where and how large emotes appear on screen, replacing the previous fixed 500×500-centered default.

## v1.5 — Global Hotkeys

### Added
- System-wide hotkeys via `tauri-plugin-global-shortcut`: Ctrl+=/Ctrl+-/Ctrl+Arrow (resize/move the character) and Alt+1–9 (fire emotes) now work regardless of which application has focus.

### Changed
- Removed the earlier local (in-window-only) keyboard handlers to avoid double-firing.

## v1.4 — Crop & Position Editor

### Added
- Built-in crop/zoom/position editor for every image added to the app (avatar or emote frames). Every exported frame is forced to identical 512×512 dimensions, guaranteeing aligned cycling regardless of source image size. Click any existing thumbnail to re-open it for adjustment.

### Fixed
- Canvas export failing with a "tainted canvas" security error — images are now loaded via a data-URL bridge instead of Tauri's asset protocol, which doesn't send permissive CORS headers.

## v1.3 — Emotes & Multi-Frame Avatar

### Added
- Pop-up emote system: independent of the avatar, centered on screen, each with its own frame set, duration, and optional hotkey.
- Multi-frame cycling for idle/talking states (blinking, alternating mouth shapes, etc.) instead of a single static image per state.
- Draggable character window (click-drag or Ctrl+Arrow keys).

### Fixed
- Faint border artifact around the "transparent" character window (Windows was adding a default window shadow).
- Settings silently resetting to defaults (including the configured microphone) when new fields were added to the settings schema without backward-compatible defaults for older saved configs.
- Emote window remaining click-blocking despite being set to click-through, and a missing OS permission grant for window dragging.

## v1.2 — Quick Wins, Presentation & Profiles

### Added
- Noise gate, adjustable mouth-hold time, always-on audio smoothing baked into the actual voice-activity decision.
- Character opacity, position lock, click-through mode, horizontal flip, rotation, drop shadow, outline.
- Keyboard scaling hotkeys, drag-and-drop image loading.
- Multiple named profiles with one-click switching; existing configs auto-migrate into a "Default" profile.

## v1.1 — Calibration & Polish

### Added
- Auto-calibrate: sets sensitivity threshold automatically from a few seconds of silence plus a few seconds of normal speech.
- Peak-hold, attack/release-smoothed VU meter.

### Changed
- Prototyped a crossfade transition between idle/talking sprites; reverted after discovering it caused visible desktop bleed-through on the transparent window mid-fade. Kept the instant swap.

## v1.0 — Initial Release

### Added
- Idle/talking sprite switching driven by real-time microphone volume (RMS + decibel-based scaling).
- Transparent, borderless, always-on-top-toggleable Character Window, capturable in OBS and TikTok Live Studio via standard Window Capture.
- Settings persistence with defensive error handling (missing mic, missing images, unsupported formats).

### Fixed (pre-release stabilization)
- Missing application icon blocking the Windows build.
- `cpal::Stream` thread-safety violation — the audio stream was moved to run entirely on its own dedicated thread, controlled via a channel, rather than being held directly in shared app state.
- Native file-picker dialog blocking Tauri's async runtime — switched to the non-blocking dialog API bridged through a channel.
- Character Window loading the wrong page due to a path living outside Tauri's configured asset root.
- Images failing to load — missing asset-protocol scope in the Tauri configuration.
- Microphone and image selections not taking effect without an app restart — a required event-listening permission had never been granted.
