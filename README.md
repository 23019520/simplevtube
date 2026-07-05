# SimpleVTube

A lightweight PNGTuber desktop app. Talk into your mic, your idle sprite swaps to a talking sprite. Capture the transparent window in OBS or TikTok Live Studio. No webcam, no rigging, no green screen.

This is the v1.0 beta scaffold, built directly from the SRS + Architecture design doc (`docs/SimpleVTube_SRS_Architecture.md`).

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

1. Click **Browse** next to Idle image → pick a PNG.
2. Click **Browse** next to Talking image → pick a PNG.
3. Select your microphone from the dropdown.
4. Adjust the Sensitivity slider while watching the live meter — set the threshold marker just above your room's background noise floor.
5. Click **Launch character**.
6. In OBS or TikTok Live Studio, add a **Window Capture** source targeting "SimpleVTube Character", enable transparency if the capture method asks for it.
7. Speak — the sprite should flip from idle to talking within ~100ms and hold briefly through short pauses.

## What's real vs. what needs tuning

This scaffold is a complete, working implementation of every FR in the SRS — but two things are flagged in the Architecture doc's Risks section (B.8) as needing your hands-on tuning once you actually run it on your machine:

- **Volume-to-percentage gain scaling** in `audio_engine.rs` (the `volume_pct` calculation) is a rough starting point. Different microphones have very different gain levels — you may need to adjust the multiplier there, or we can add an auto-calibration step if the default range feels off.
- **Always-on-top / transparency behavior** can differ subtly across Windows and macOS. If the Character Window shows a black background instead of transparent in OBS, that's the #1 known cross-platform risk called out in the architecture doc (B.8.1) — flag it and we'll debug it together against your specific OS/OBS version.

## Project structure

See `docs/SimpleVTube_SRS_Architecture.md`, Part C, for the full directory map and the exact file-to-file communication contracts this code implements.
