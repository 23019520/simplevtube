# SimpleVTube

A lightweight PNGTuber desktop app. Talk into your mic, your idle sprite swaps to a talking sprite. Capture the transparent window in OBS or TikTok Live Studio. No webcam, no rigging, no green screen.

This is v1.2, built from the SRS + Architecture design doc (`docs/SimpleVTube_SRS_Architecture.md`) plus Phases 1, 2, and 4 of the v2 roadmap (`docs/v2-roadmap.md`).

## What's new in v1.5
- **System-wide global hotkeys** — Ctrl+=/Ctrl+-/Ctrl+Arrow (resize/move the character) and Alt+1 through Alt+9 (fire emotes) now work anywhere on your PC, even while a game, OBS, or any other app has focus. Previously these only worked while the Control Window itself was focused.
- **Honest tradeoff to know about:** these are genuinely global — Ctrl+= and Ctrl+Arrow are common shortcuts in browsers and some other apps (zoom, text navigation). While SimpleVTube is running, it takes over those combos system-wide. There's no per-shortcut disable in this version; if one conflicts with something you use elsewhere, that's the current cost of it working outside the app at all.

- **Crop/position editor** — every image you add (avatar frames or emote frames) now opens in a built-in editor first: drag to position, scroll or use the slider to zoom, with a grid overlay for alignment reference. Every exported frame is saved at the exact same 512×512 dimensions, guaranteeing perfectly aligned cycling between frames regardless of what size or aspect ratio your original images were.
- **Click any existing thumbnail to re-crop it** — no need to remove and re-add if you want to nudge the positioning later.
- Your original image files are never modified — edited frames are saved as new files in the app's own data folder, so cropping is fully non-destructive.

- **Pop-up emotes** — separate from the avatar entirely, shown centered on screen. Add as many emotes as you want, each with its own image frames, duration, and an optional Alt+digit hotkey. Trigger with the "Test" button or the hotkey.
- **Multi-frame avatar states** — idle and talking can each cycle through multiple images now (e.g. blinking, alternating mouth shapes), not just one static picture each. Add frames the same way as before (Browse or drag-and-drop); a single frame behaves exactly like before.
- **Draggable character** — click and drag the character directly, or use Ctrl+Arrow keys to nudge it. Both respect "Lock position & size."
- **Fixed the transparent-frame artifact** — Windows was adding a faint border to the borderless character window; disabled.

- **Noise gate, adjustable mouth hold time, and always-on audio smoothing** — tune out background noise and chatter without editing code
- **Auto-calibrate** — sets your sensitivity threshold automatically from a few seconds of quiet + a few seconds of normal talking
- **Peak-hold VU meter** — see your loudest recent moment, not just the current instant
- **Character opacity, lock, click-through, flip, rotation, drop shadow, outline** — all under the "Advanced" section
- **Scaling hotkeys** — Ctrl+= / Ctrl+- to resize the character without touching the mouse
- **Drag-and-drop image loading** — drop a PNG onto the idle/talking row instead of only using Browse
- **Multiple profiles** — save separate character/mic/effect setups and switch between them with one click. Existing v1/v1.1 configs are migrated automatically into a "Default" profile — nothing is lost.

## Prerequisites

You'll need these installed once, before first run:

1. **Rust** — https://rustup.rs (installs `cargo`)
2. **Node.js** (LTS) — https://nodejs.org
3. **Tauri CLI** — installed automatically via the project's `devDependencies`, but you also need the Tauri OS-level prerequisites:
   - **Windows:** Microsoft C++ Build Tools + WebView2 (usually already present on Win10/11)
   - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
   - **Linux:** see https://tauri.app/start/prerequisites/ (webkit2gtk, etc.)

## First run

```bash
npm install
npm run dev
```

This launches the Control Window. The Character Window stays hidden until you click **Launch character**.

## Getting to a working beta

1. Click **Browse** next to Idle image → pick a PNG (or drag-and-drop one onto the row).
2. Click **Browse** next to Talking image → pick a PNG.
3. Select your microphone from the dropdown.
4. Click **Auto-calibrate** and follow the prompts, or manually adjust the Sensitivity slider while watching the VU meter.
5. Click **Launch character**.
6. In OBS or TikTok Live Studio, add a **Window Capture** source targeting "SimpleVTube Character", enable transparency if the capture method asks for it.
7. Speak — the sprite should flip from idle to talking within ~100ms and hold briefly through short pauses.

## What's real vs. what needs tuning

This scaffold is a complete, working implementation of every FR in the SRS plus Phases 1/2/4 of the v2 roadmap — but a few things are worth your own hands-on verification once you run it:

- **Volume-to-percentage scaling** in `audio_engine.rs` uses a decibel-based formula that should work well across most mics, but auto-calibrate exists specifically because every microphone's gain is different — use it if talking doesn't reliably cross the threshold.
- **Always-on-top / transparency behavior** can differ subtly across Windows and macOS. If the Character Window shows a black background instead of transparent in OBS, flag it and we'll debug it together against your specific OS/OBS version.
- **Drag-and-drop** relies on Tauri's native file-drop bridge, which is a slightly less battle-tested path than the Browse-button dialog flow — if a drop doesn't register, Browse still always works as the fallback.

## Not yet built

Phase 3 of the v2 roadmap (blinking, breathing, idle bobbing, multi-frame talking, animated GIF/APNG/WebP support) is a genuinely separate animation-engine subsystem and hasn't been started — see `docs/v2-roadmap.md` for the full breakdown. Phase 5 (expression hotkeys, emote wheel) is intentionally not scoped yet.

## Project structure

See `docs/SimpleVTube_SRS_Architecture.md`, Part C, for the full directory map and the exact file-to-file communication contracts this code implements.
