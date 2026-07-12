<div align="center">

# SimpleVTube

**A lightweight PNGTuber desktop app.** Talk into your mic, your idle sprite swaps to a talking sprite. No webcam, no rigging, no green screen — just a transparent character window you capture straight into OBS or TikTok Live Studio.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/Built%20with-Tauri%202-24C8DB)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Backend-Rust-orange)](https://www.rust-lang.org)

<!-- Add a demo GIF or screenshot here before publishing -->

</div>

---

## Why this exists

Most PNGTuber software either costs money, comes with far more complexity than a simple mic-reactive avatar needs, or both. SimpleVTube is a personal tool, built for one streaming setup, with a hard rule borrowed from the original spec: **every feature has to make streaming easier, not more complicated.**

It's been used across multiple live streams (TikTok Live Studio and OBS) and is actively maintained for personal use — public and open to issues or ideas, but scoped intentionally rather than chasing feature parity with larger VTuber apps.

## Development process

Built through an iterative, spec-first workflow with AI-assisted implementation (Claude, Anthropic) — I wrote the requirements, drove every feature decision, tested each build on real hardware across real streams, and diagnosed issues from actual runtime behavior the AI had no way to observe on its own (OBS capture quirks, microphone gain differences, Windows-specific rendering bugs). The AI wrote code; verifying it actually worked was on me.

I don't think there's anything to hide about that. Knowing how to direct, specify, and rigorously test AI-assisted development is a real skill, and this project — SRS and architecture docs written before a line of code existed, a full changelog of real bugs found and fixed through actual use, not just features bolted on — is as much a demonstration of that skill as of the app itself.

## Features

**Core**
- Real-time mic-driven idle/talking sprite switching (RMS + decibel-based volume detection)
- Transparent, borderless, always-on-top character window — captures cleanly in OBS and TikTok Live Studio
- Multi-frame cycling per state (blinking, alternating mouth shapes) instead of one static image
- Built-in crop/position editor with onion-skinning, so every frame aligns pixel-perfectly regardless of source image size

**Audio**
- Noise gate, adjustable mouth-hold time, live smoothing
- One-click auto-calibration from a few seconds of silence + speech
- Optional Automatic Gain Control for mics with inconsistent loudness
- Live scrolling waveform + peak-hold VU meter

**Character**
- Drag to reposition, or move/resize with global hotkeys — works even while another app has focus
- Opacity, lock, click-through, flip, rotation, drop shadow, outline
- Real spring-physics reactive bounce and idle breathing animation (not CSS transitions)
- Snaps to screen edges/center when dragged close

**Workflow**
- Pop-up emotes with per-emote frames, duration, and hotkeys
- Multiple named profiles, one-click switching
- Command palette (Ctrl+K) — fuzzy search for every action in the app
- Undo/redo across every settings change
- Stream Mode — collapses the UI to essentials while you're live

## Tech stack

- **[Tauri 2](https://tauri.app)** — Rust backend, native webview frontend, chosen specifically over Electron to hit tight RAM/startup targets
- **Rust** — audio pipeline ([`cpal`](https://github.com/RustAudio/cpal)), settings persistence, window management, global hotkeys
- **Vanilla HTML/CSS/JS** — no frontend framework or bundler; the whole UI is hand-rolled against Tauri's injected API

## Getting started

**Prerequisites:** [Rust](https://rustup.rs), [Node.js](https://nodejs.org) (LTS), and the [Tauri OS-level prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run dev
```

This launches the Control Window. The Character Window stays hidden until you click **Launch character**.

**First-run checklist:**
1. Add idle and talking frames (Browse or drag-and-drop) — each opens in the built-in crop editor.
2. Select your microphone and click **Auto-calibrate**.
3. Click **Launch character**.
4. In OBS/TikTok Live Studio, add a **Window Capture** source targeting "SimpleVTube Character." If OBS shows a black background instead of transparency, switch the capture Method to the Windows 10+ (Windows Graphics Capture) option and enable "Allow Transparency."

## Documentation

This project was built spec-first — the original requirements and architecture documents are kept alongside the code, not thrown away after the fact:

- **[`docs/SimpleVTube_SRS_Architecture.md`](docs/SimpleVTube_SRS_Architecture.md)** — the original Software Requirements Specification and system architecture, including the full directory map and file-to-file communication contracts
- **[`docs/v2-roadmap.md`](docs/v2-roadmap.md)** — feature backlog, triaged by actual implementation complexity rather than just topic
- **[`docs/v1.12-final-sprint.md`](docs/v1.12-final-sprint.md)** — the closing sprint plan, including what was deliberately left out and why
- **[`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)** — practical fixes for OBS transparency, mic calibration, and other issues found during real use
- **[`CHANGELOG.md`](CHANGELOG.md)** — full version history

## Known limitations

A few things are intentionally out of scope for now (see the roadmap doc for the full reasoning):

- No animated GIF/APNG/sprite-sheet playback — the animation model is discrete frame-cycling, not decoded animated files
- No OBS WebSocket integration, virtual camera, or NDI/Spout output
- No Stream Deck/MIDI input, cloud sync, or plugin system

## License

[MIT](LICENSE) — see the file for details.
