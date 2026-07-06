# SimpleVTube — v2 Feature Roadmap

Source: v2 wishlist (29 items). Grouped by what they actually require to build, not just topic — some are a 10-line change, some mean rethinking core architecture.

## ✅ Already done (v1 / v1.1 / v1.2)
- Automatic microphone calibration
- Always-on-top toggle
- Character resize (native window resize)
- **Phase 1 — Quick wins** (shipped in v1.2): noise gate, audio smoothing (now baked into the actual VAD decision, not just the meter), peak-capable smoothed meter, adjustable mouth hold time, opacity, lock position, click-through mode, flip horizontally, drag-and-drop image loading
- **Phase 2 — Character presentation** (shipped in v1.2): shadows, outlines, rotation, scaling hotkeys (Ctrl+=/Ctrl+-)
- **Phase 4 — Profiles** (shipped in v1.2): multiple named profiles with automatic migration from older configs, one-click switching, create/delete

## 🟠 Phase 3 — Animation system (large — this is a real architecture change)
Everything below assumes a character can have **multiple frames per state**, not one static PNG. That means: a new sprite-sequence data model, a frame-timing/playback engine, and settings UI to manage frame lists. This is the biggest single piece of work on the list, and several other items depend on it landing first:
- Random blinking
- Breathing animation
- Idle bobbing
- Multiple talking frames
- Smooth mouth transitions (open/close in-betweens)
- Sprite sheet animations
- Idle animation cycles
- Talking animation cycles
- Animated GIF / APNG / animated WebP support (feeds frames into the same engine)

## 🔴 Phase 4 — Profiles (medium-large — settings model rework)
- Multiple character profiles (settings_manager needs to store a list of named configs, not one flat config)
- One-click profile switching

## 🔴 Phase 5 — Expression system (largest, most speculative)
- Expression hotkeys
- Emote wheel

This needs a whole new state model beyond binary Idle/Talking — discrete triggerable expressions, a hotkey-registration system, and a UI for it. Worth scoping properly on its own once Phase 3's animation engine exists, since expressions are really "animation frames triggered manually instead of by voice."

## Recommendation
Do Phase 1 as v1.2 — it's a week of feature-add-a-day pace, nothing risky, and every item ships independently. Then decide whether Phase 2 (visual polish) or Phase 3 (animation engine) matters more to you before touching Phases 4–5, since those are genuinely bigger commitments.
