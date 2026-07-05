# SimpleVTube — Software Requirements Specification & Architecture Design

**Document Version:** 1.0
**Based on:** SimpleVTube User Specification Document v1.0
**Status:** Draft for engineering review

---

## Part A — 📐 Software Requirements Specification (SRS)

### A.1 Introduction

#### A.1.1 Purpose
This document translates the SimpleVTube User Specification into a formal, implementable Software Requirements Specification. It defines functional and non-functional requirements at an engineering level of detail, along with system constraints, interfaces, and acceptance criteria suitable for design and test planning.

#### A.1.2 Scope
SimpleVTube is a lightweight, offline, cross-platform desktop application that renders a transparent, borderless "character window" whose displayed image toggles between an **idle** sprite and a **talking** sprite based on real-time microphone volume analysis. The character window is designed to be captured by third-party streaming software (OBS, TikTok LIVE Studio) via standard window/game capture methods.

Out of scope for v1.0: animation, blinking/breathing, multi-frame talking states, expression/emotion systems, AI-driven features, streaming-platform integrations, and non-PNG asset formats. (See User Spec §10.)

#### A.1.3 Definitions

| Term | Definition |
|---|---|
| Idle State | The character is not detected as speaking; idle sprite is displayed |
| Talking State | Microphone volume exceeds threshold; talking sprite is displayed |
| VAD | Voice Activity Detection — the volume-threshold logic determining Idle/Talking |
| Character Window | The transparent, borderless, always-capturable render surface |
| Control Window | The main application window with settings UI |
| Hold Time | Minimum duration a state must persist before switching back, to prevent flicker |

#### A.1.4 References
- SimpleVTube User Specification Document, v1.0

---

### A.2 Overall Description

#### A.2.1 Product Perspective
SimpleVTube is a standalone desktop application with two windows (Control, Character) sharing a single process and audio pipeline. It has no server component, no account system, and no network dependency at runtime.

#### A.2.2 User Classes
- **Primary:** Streamers/creators with no technical background — must succeed with zero documentation.
- **Secondary:** Power users who may want to tune sensitivity precisely or manage multiple presets across sessions.

#### A.2.3 Operating Environment
- **OS targets (v1.0):** Windows 10/11 (primary), macOS 12+ (secondary). Linux is a stretch goal, not a v1.0 commitment — window transparency and always-on-top behavior are the highest-risk items cross-platform.
- **Hardware:** Any modern consumer laptop/desktop; no GPU requirement.
- **Runtime dependency:** None required from the user (self-contained installer/bundle).

#### A.2.4 Design & Implementation Constraints
- Must run fully offline (NFR, User Spec §5).
- Must stay under 100 MB RAM and start in under 3 seconds (User Spec §5).
- Must not require the user to install a runtime, codec, or driver.
- Character window must be OS-native transparent (not a chroma-key trick), since "no green screen required" is an explicit requirement.

#### A.2.5 Assumptions and Dependencies
- User has at least one working input audio device.
- User supplies their own PNG artwork; no bundled asset library in v1.0.
- OBS/TikTok Live Studio compatibility is validated against their standard Window Capture / Game Capture source types — no custom OBS plugin is built for v1.0 (explicitly deferred, User Spec §10).

---

### A.3 Functional Requirements (Engineering-Level)

Each requirement below expands the corresponding FR from the User Specification into testable acceptance criteria.

#### FR-001 — Character Image Import
- **FR-001.1** The system shall provide file-picker controls to import an Idle Image and a Talking Image.
- **FR-001.2** The system shall accept only `.png` files in v1.0; other extensions shall be rejected with the error defined in §A.3 Error Handling.
- **FR-001.3** The system shall validate that the selected file is a decodable PNG before accepting it (not just extension matching).
- **FR-001.4** Images shall be displayed at native resolution by default, with the Character Window resizable independent of source resolution (scaled, aspect-ratio preserved).
- **Acceptance:** Importing a valid PNG updates the preview in the Control Window within 200ms; importing a corrupt or non-PNG file produces the "Unsupported image format" error without crashing the app.

#### FR-002 — Microphone Detection & Selection
- **FR-002.1** On launch, the system shall enumerate all available audio input devices via the OS audio API.
- **FR-002.2** The device list shall be presented in a dropdown in the Control Window.
- **FR-002.3** The system shall re-enumerate devices if the user opens the dropdown (to catch hot-plugged devices) without requiring an app restart.
- **FR-002.4** If the previously selected microphone (from persisted settings) is unavailable at launch, the system shall fall back to the OS default input device and notify the user.
- **Acceptance:** Plugging/unplugging a USB mic and reopening the dropdown reflects the change without restart.

#### FR-003 — Voice Activity Detection (VAD)
- **FR-003.1** The system shall continuously sample microphone input and compute a rolling volume level (e.g., RMS amplitude) at a minimum of 20 Hz.
- **FR-003.2** The system shall compare the computed volume against a user-configurable threshold (0–100 scale) to classify state as Idle or Talking.
- **FR-003.3** The system shall apply a debounce/hold time (recommended default: 150–250 ms) before switching from Talking back to Idle, to prevent flicker on short pauses between words.
- **FR-003.4** The sensitivity slider in the Control Window shall map linearly (or logarithmically, pending UX testing) to the threshold value and shall take effect in real time without requiring a restart.
- **Acceptance:** Speaking at normal volume reliably triggers Talking state within 100ms; stopping speech returns to Idle within the configured hold time; background room noise at rest does not cause false triggers at default sensitivity.

#### FR-004 — Character Window
- **FR-004.1** The system shall render a second, independent OS-level window with a transparent background (alpha channel, not color-key).
- **FR-004.2** This window shall be borderless (no title bar, no OS chrome) and shall display only the current sprite.
- **FR-004.3** The window shall support freeform resize via drag handles or corner-drag while borderless (custom resize handling, since OS-native resize grips require a border).
- **FR-004.4** The window's last position and size shall be persisted (see FR-006) and restored on next launch, clamped to visible screen bounds in case of resolution/monitor changes.
- **FR-004.5** The Character Window shall support an "always on top" mode, toggleable from the Control Window.
- **Acceptance:** The Character Window shows no OS border/shadow artifacts when captured; resizing persists across restarts; window reopens on-screen even if the previous monitor is disconnected.

#### FR-005 — OBS / Capture Software Compatibility
- **FR-005.1** The Character Window shall be enumerable as a standard top-level window so it appears in OBS's Window Capture source list by its process/window title.
- **FR-005.2** Transparency shall be preserved through capture (validated specifically against OBS Window Capture with "Allow Transparency" and against TikTok Live Studio's equivalent capture mode).
- **FR-005.3** No custom capture plugin, virtual camera, or NDI dependency shall be required for basic capture to work.
- **Acceptance:** A test build is manually validated capturing correctly in both OBS (Windows/macOS) and TikTok Live Studio with background elements visibly transparent (not black).

#### FR-006 — Settings Persistence
- **FR-006.1** The system shall persist, across restarts: last selected microphone (by stable device ID, not index), sensitivity threshold, idle image path, talking image path, Character Window position, Character Window size, and always-on-top state.
- **FR-006.2** Settings shall be stored in a local, human-readable config file (e.g., JSON) in the OS-standard app-data directory.
- **FR-006.3** If a referenced image file has moved or been deleted since last launch, the system shall clear that field and prompt re-selection rather than crash.
- **Acceptance:** Closing and reopening the app restores the exact prior configuration; a moved image file triggers the "Please choose an idle/talking image" prompt rather than a fatal error.

### A.4 Error Handling Requirements

| Condition | User-Facing Message | System Behavior |
|---|---|---|
| No microphone detected | "No microphone detected." | Disable Launch Character button; allow retry/re-scan |
| Idle image missing | "Please choose an idle image." | Block character launch until resolved |
| Talking image missing | "Please choose a talking image." | Block character launch until resolved |
| Image fails to decode | "Unsupported image format." | Reject file, retain previous valid selection if any |

All error states must be non-fatal: the Control Window must remain responsive and never crash the process.

### A.5 Non-Functional Requirements

| ID | Requirement | Target |
|---|---|---|
| NFR-001 | Cold start time | < 3 seconds |
| NFR-002 | Idle memory footprint | < 100 MB RAM |
| NFR-003 | Idle CPU usage | Minimal; audio polling should not pin a core |
| NFR-004 | Continuous runtime stability | No memory growth/degradation over multi-hour sessions |
| NFR-005 | Network dependency | None; fully offline-capable |
| NFR-006 | Setup time (first-run to live) | < 2 minutes without documentation |
| NFR-007 | Accessibility | Full keyboard navigation, UI scaling, dark mode support |

### A.6 Accessibility Requirements
- All Control Window actions (file browse, dropdown selection, slider adjustment, buttons) must be reachable and operable via keyboard (Tab/Shift+Tab, Enter, Arrow keys).
- UI must support OS-level or in-app scaling for visibility.
- A dark mode theme must be available and togglable or auto-detected from OS theme.

### A.7 Success Criteria (Traceable to User Spec §11)
The SRS is considered satisfied when a user can, without documentation, in under two minutes: install → load idle/talking sprites → select a mic → speak and observe correct state switching → capture the transparent Character Window in OBS or TikTok Live Studio.

---

## Part B — 🏛️ Architecture Design

### B.1 Architectural Goals
- **Lightweight & responsive:** low idle footprint, no heavyweight rendering engine needed for two static sprites.
- **Two-window, single-process model:** simplicity over microservice-style separation; avoids IPC complexity for a single-user local app.
- **Cross-platform native windowing:** true alpha transparency, not a compositing hack — this drives the framework choice below.
- **Modular audio pipeline:** VAD logic must be swappable/tunable without touching UI code.

### B.2 High-Level System Architecture

```
                        ┌─────────────────────────────┐
                        │        Application Core      │
                        │        (main process)        │
                        └───────────────┬───────────────┘
                                        │
        ┌───────────────────────────────┼───────────────────────────────┐
        │                               │                               │
┌───────▼────────┐            ┌─────────▼─────────┐            ┌────────▼────────┐
│  Audio Engine   │            │  Settings Manager  │            │  Window Manager  │
│  - device enum  │◄──────────►│  - JSON config I/O │◄──────────►│  - Control Window│
│  - volume sample│   state    │  - persistence     │   state    │  - Character Win │
│  - VAD/threshold│   events   │  - defaults/recovery│  events    │  - resize/position│
└───────┬─────────┘            └────────────────────┘            └────────┬─────────┘
        │  Idle/Talking state change events                               │
        └──────────────────────────────►  Renderer  ◄──────────────────────┘
                                        (sprite swap)
```

**Component responsibilities:**

- **Application Core** — process lifecycle, startup sequencing, dependency wiring, error boundary/crash guarding.
- **Audio Engine** — enumerates input devices, opens an audio stream, computes rolling RMS volume, applies threshold + hold-time debounce, emits `state-changed` events (Idle ↔ Talking).
- **Settings Manager** — reads/writes the local JSON config, validates paths on load (image existence, device availability), supplies defaults on first run or recovery on corruption.
- **Window Manager** — owns both OS windows: the Control Window (standard UI chrome) and the Character Window (transparent, borderless, always-on-top capable, custom resize handling).
- **Renderer** — subscribes to `state-changed` events and swaps the displayed sprite in the Character Window; purely reactive, no business logic.

### B.3 Recommended Technology Stack

Two viable stacks were evaluated:

| Criterion | Electron + Web Audio API | Native (e.g., Tauri + Rust, or C++/Qt) |
|---|---|---|
| True window transparency | Supported, well-documented per-OS | Supported, more manual per-OS work |
| RAM footprint (<100MB target) | Harder to hit — Chromium overhead is real risk | Easier to hit natively |
| Dev speed / iteration | Fast, huge ecosystem, easy PNG/UI handling | Slower, more platform-specific code |
| Cross-platform audio input | `getUserMedia` + Web Audio API, straightforward | Needs platform audio bindings (WASAPI/CoreAudio) or a cross-platform crate/lib |
| Packaging/installer | Mature tooling (electron-builder) | Mature but more setup (per-OS) |

**Recommendation: Tauri (Rust backend + lightweight webview frontend).**
This targets the NFR-001/NFR-002 constraints (fast start, <100MB RAM) far more reliably than Electron, while still allowing the Control Window UI to be built with familiar web tech (HTML/CSS/JS) inside Tauri's native webview — avoiding the Chromium-per-app overhead that makes Electron's memory budget hard to hit. Tauri also has first-class support for transparent, borderless, always-on-top windows on Windows and macOS, and an actively maintained audio ecosystem via Rust crates (e.g., `cpal` for cross-platform audio input).

*If the team's existing skillset is strongly JS/TS-only with no Rust appetite, Electron remains a fallback — but the 100MB RAM target should be treated as at-risk and re-scoped with stakeholders if that path is chosen.*

### B.4 Audio Pipeline Design (Detail)

```
Mic Input
   │
   ▼
[cpal audio stream] ──► raw PCM samples
   │
   ▼
[RMS volume calculator]  (rolling window, ~20-50ms frames)
   │
   ▼
[Threshold comparator]  (user-set sensitivity, 0-100 → amplitude threshold)
   │
   ▼
[Hold-time debounce]  (prevents rapid Idle/Talking flapping)
   │
   ▼
state-changed event ──► Renderer swaps sprite
```

Design notes:
- Volume calculation and thresholding run on a dedicated audio thread, decoupled from UI thread, to avoid audio glitches or UI jank affecting detection latency.
- The debounce/hold-time logic is the key lever for perceived "naturalness" — this should be exposed as an advanced/optional setting post-v1, but hardcoded to a sensible default (~200ms) for v1 to keep the UI simple per the "keep it simple" philosophy.

### B.5 Window Management Design

- **Control Window:** standard OS window with title bar, resizable, houses all settings UI described in User Spec §6.
- **Character Window:** created with OS-level flags for `transparent: true`, `decorations: false` (borderless), `always_on_top: configurable`. Because borderless windows lose native OS resize handles, a custom edge/corner hit-test region is implemented in the Window Manager to allow drag-resize while borderless.
- Both windows' geometry (position/size) is persisted via the Settings Manager on every move/resize event (debounced to avoid excessive disk writes — e.g., write on resize-end, not per-frame).

### B.6 Data Model (Settings File)

```json
{
  "microphoneDeviceId": "string",
  "sensitivityThreshold": 42,
  "idleImagePath": "string",
  "talkingImagePath": "string",
  "characterWindow": {
    "x": 100,
    "y": 100,
    "width": 400,
    "height": 400,
    "alwaysOnTop": true
  },
  "theme": "dark"
}
```

Stored at the OS-standard app-config location (e.g., `%APPDATA%/SimpleVTube/config.json` on Windows, `~/Library/Application Support/SimpleVTube/config.json` on macOS).

### B.7 Error & Recovery Strategy
- All file/device I/O is wrapped defensively; failures degrade to a re-prompt state (per §A.4) rather than throwing unhandled exceptions.
- Config file corruption falls back to hardcoded defaults, with the corrupt file renamed/backed up rather than silently overwritten, to aid support/debugging.
- Audio device disconnection mid-session (e.g., USB mic unplugged while live) should surface a non-blocking notification and fall back to system default input rather than freezing the VAD pipeline.

### B.8 Risks & Open Questions
1. **Cross-platform transparent/borderless/always-on-top behavior** is the single highest technical risk — needs an early spike/prototype before committing further engineering time, per platform.
2. **RMS-threshold VAD is naive** and will need real-world tuning against different mic types (USB condenser vs. laptop built-in) to avoid false triggers from keyboard noise, breathing, or fan noise — recommend a short internal beta before wider release.
3. **Linux support** is explicitly not committed for v1.0 given transparency/window-manager fragmentation (compositor-dependent); revisit post-v1 based on demand.

---

---

## Part C — 🗂️ Project Directory Structure & File Communication Map

This section exists specifically so that v1 implementation has **no surprises**: every file's job, and exactly which other files it talks to, is defined up front. This assumes the recommended stack (Tauri: Rust backend in `src-tauri/`, web-based UI in `src/`).

### C.1 Directory Tree

```
simplevtube/
├── src/                          # Frontend (Control Window UI) — HTML/CSS/JS
│   ├── index.html                # Control Window markup (Section 6 layout)
│   ├── main.js                   # UI logic: wires buttons/dropdowns to Tauri commands
│   ├── styles.css                # Light/dark theme, layout, scaling
│   ├── character-window/         # Minimal render surface for the Character Window
│   │   ├── index.html            # Just an <img> tag — no UI, no chrome
│   │   └── render.js             # Subscribes to state-changed events, swaps <img src>
│   └── assets/
│       └── icons/                # App icon, UI icons (not user sprites)
│
├── src-tauri/                    # Backend (Rust) — the "Application Core"
│   ├── src/
│   │   ├── main.rs               # Entry point: boot sequence, window creation, wiring
│   │   ├── audio_engine.rs       # Mic enumeration, RMS volume calc, VAD, debounce
│   │   ├── settings_manager.rs   # Read/write config.json, defaults, recovery
│   │   ├── window_manager.rs     # Character Window creation, transparency, resize/drag, always-on-top
│   │   ├── events.rs             # Shared event/state types (StateChanged, SettingsUpdated, DeviceError)
│   │   └── commands.rs           # Tauri #[command] functions exposed to the frontend (see C.3)
│   ├── icons/                    # Packaged app icons per OS
│   └── tauri.conf.json           # Window definitions (Control + Character), permissions, bundle config
│
├── user-data/ (runtime, not shipped)
│   └── config.json               # Generated at runtime in OS app-data dir — see B.6
│
└── docs/
    └── SimpleVTube_SRS_Architecture.md   # this document
```

> **Note (post-beta correction):** `character-window/` was originally planned as a sibling of `src/`, but Tauri's dev/asset server only serves files inside the configured `frontendDist` root (`src/`). A sibling path can't be resolved and silently falls back to the wrong window content. It now lives at `src/character-window/` for that reason — same file contracts as originally designed, just nested correctly.

### C.2 Who Talks to Whom (Component Communication Map)

```
┌───────────────────┐   invokes commands    ┌────────────────────────┐
│  src/main.js        │ ─────────────────────►│  commands.rs            │
│  (Control Window)   │                        │  (Tauri command layer)  │
│                     │ ◄─────────────────────│                          │
└─────────┬───────────┘   returns results /    └───────────┬──────────────┘
          │                emits events                     │
          │ listens to                                      │ calls into
          │ "state-changed"                                 │
          │ "settings-updated"                               │
          │ "device-error"                                  ▼
          │                                    ┌────────────────────────┐
          │                                    │ audio_engine.rs         │
          │                                    │ settings_manager.rs     │
          │                                    │ window_manager.rs       │
          │                                    └───────────┬─────────────┘
          │                                                │ emits
          │                                                ▼
          │                                    ┌────────────────────────┐
          └────────────────────────────────────►  events.rs (bus)        │
                                                └───────────┬──────────────┘
                                                            │ emits to
                                                            ▼
                                                ┌────────────────────────┐
                                                │ character-window/        │
                                                │ render.js                 │
                                                │ (Character Window)        │
                                                └────────────────────────┘
```

### C.3 Concrete File-to-File Contracts

This is the part that removes ambiguity during implementation — each row is a promise one file makes to another.

| From | To | Mechanism | Payload / Contract |
|---|---|---|---|
| `src/main.js` | `commands.rs` | Tauri `invoke()` call | `select_idle_image()`, `select_talking_image()`, `list_microphones()`, `set_microphone(id)`, `set_sensitivity(value)`, `launch_character()`, `hide_character()`, `set_always_on_top(bool)` |
| `commands.rs` | `audio_engine.rs` | direct Rust function call | starts/stops audio stream, applies new threshold, returns device list |
| `commands.rs` | `settings_manager.rs` | direct Rust function call | persist updated field; read config on startup |
| `commands.rs` | `window_manager.rs` | direct Rust function call | create/show/hide Character Window, toggle always-on-top |
| `audio_engine.rs` | `events.rs` | emits `state-changed` event | `{ state: "idle" \| "talking", timestamp }` |
| `audio_engine.rs` | `events.rs` | emits `device-error` event | `{ reason: "no-device" \| "disconnected" }` |
| `settings_manager.rs` | `events.rs` | emits `settings-updated` event | full current config snapshot (used to sync UI after recovery/defaults load) |
| `events.rs` | `src/main.js` | Tauri `listen()` (event bridge) | Control Window updates status dot ("● Listening"), re-renders previews, shows error banners |
| `events.rs` | `character-window/render.js` | Tauri `listen()` (event bridge) | swaps `<img src>` between idle/talking image paths on `state-changed` |
| `main.rs` | everything | boot sequence only | on startup: `settings_manager` loads config → `window_manager` restores Character Window geometry → `audio_engine` opens last-used mic (or default) → windows shown |
| `window_manager.rs` | `settings_manager.rs` | direct Rust function call, debounced | on move/resize-end, persist new `x, y, width, height` |

### C.4 Boot Sequence (No-Surprises Startup Order)

1. `main.rs` starts → creates Control Window (hidden until ready) and initializes `events` bus.
2. `settings_manager.rs` loads `config.json` (or defaults if missing/corrupt) → emits `settings-updated`.
3. `src/main.js` receives `settings-updated` → populates dropdowns, sliders, image previews from saved state.
4. `audio_engine.rs` attempts to open the persisted microphone device ID → falls back to system default if unavailable → emits `device-error` if none exist at all.
5. `window_manager.rs` restores Character Window position/size from config (if the prior session had it open) — otherwise Character Window stays closed until the user clicks **Launch Character**.
6. Control Window becomes visible and interactive; status indicator reflects real mic state ("● Listening" vs error state).

### C.5 Why This Matters for "No Surprises" in v1
- **Single source of truth for state:** `events.rs` is the only channel through which state crosses the Rust/JS boundary — no file reaches into another's internals directly, which keeps debugging linear (trace one event bus, not five ad-hoc callbacks).
- **The Character Window is intentionally dumb:** `render.js` does nothing but listen and swap an image `src`. All decision-making (VAD, thresholds, debounce) lives in `audio_engine.rs`. This means the render surface can never desync from logic, and a bug in the Character Window can only ever be a rendering bug, never a logic bug.
- **Settings changes are always round-tripped through `settings_manager.rs`**, never written directly by the UI or the audio engine — this guarantees `config.json` is always the single consistent snapshot described in B.6, with no file writing it from two places at once.

---

*This document should be treated as a living artifact — update Parts A/B/C together as scope evolves, and keep the "Future Features" boundary (User Spec §10) explicit in any revision to prevent silent scope creep.*
