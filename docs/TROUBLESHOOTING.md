# Troubleshooting

Practical fixes for issues that came up during real development and streaming use.

## OBS shows a black background instead of transparency

This is a known issue with OBS's older Window Capture method and modern WebView-rendered apps (SimpleVTube included) — it's not unique to this project. TikTok Live Studio capturing correctly while OBS doesn't is a strong signal the window itself really is transparent; this is purely an OBS capture-settings issue.

1. **Switch the capture method.** Right-click the source → Properties → change "Method" from Automatic/BitBlt to **"Windows 10 (1903 and up)"** (Windows Graphics Capture). The older method can't read transparency from modern WebView-rendered windows at all.
2. **Enable "Allow Transparency"** if the checkbox appears alongside that method.
3. **Try Game Capture instead of Window Capture**, with "Allow Transparency" checked — this is the officially recommended method for other WebView/game-engine-rendered VTuber apps on Windows, and sometimes succeeds where Window Capture doesn't.
4. **Check for a dual-GPU laptop issue.** If OBS and SimpleVTube are rendering on different GPUs (common with integrated + dedicated graphics), transparency capture can render solid black. In Windows Settings → Display → Graphics, set both `obs64.exe` and `simplevtube.exe` to "High performance."
5. Confirm with an actual recording or private test stream, not just the small OBS preview thumbnail, before going live.

## Talking never reliably triggers, even after auto-calibrating

Every microphone has different gain characteristics. Try, in order:
1. Re-run **Auto-calibrate** while speaking at your actual streaming volume, not a quiet test voice.
2. Manually lower the **Sensitivity** slider while watching the VU meter — the threshold marker should sit just above your room's resting noise floor.
3. Enable **Automatic Gain Control** (Advanced section) if your mic tends to run quiet even after calibration.

## Prerequisites by platform

- **Windows:** Microsoft C++ Build Tools + WebView2 (usually already present on Windows 10/11)
- **macOS:** Xcode Command Line Tools (`xcode-select --install`)
- **Linux:** see the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/) (webkit2gtk, etc.)

## Drag-and-drop doesn't register an image

Drag-and-drop relies on Tauri's native file-drop bridge, which is a slightly less battle-tested path than the Browse-button dialog flow. If a drop silently does nothing, the Browse button always works as a reliable fallback.

## Settings seem to have reset after an update

If you're updating from a build prior to v1.3, an early version had a bug where new required settings fields on an old config file could cause the whole file to be treated as corrupt and reset to defaults (see [CHANGELOG.md](../CHANGELOG.md), v1.3). This is fixed as of v1.3 — old configs now fall back to sensible defaults for any *individual* missing field rather than resetting everything. If you're on a build from after that fix and still see this, please open an issue.
