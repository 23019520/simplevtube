// main.js — Control Window logic.
//
// Only talks to the backend via invoke() (commands.rs) and listen()
// (events.rs). Never assumes state on its own — always reflects what the
// backend broadcasts, per Architecture doc C.5 ("single source of truth").
//
// NOTE: this project has no frontend bundler (no Vite/webpack), so ES
// module specifiers like "@tauri-apps/api/core" can't be resolved by the
// browser directly. Instead we use Tauri's injected window.__TAURI__
// global (enabled via "withGlobalTauri": true in tauri.conf.json).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const el = {
  profileSelect: document.getElementById("profile-select"),
  btnNewProfile: document.getElementById("btn-new-profile"),
  btnDeleteProfile: document.getElementById("btn-delete-profile"),
  idlePath: document.getElementById("idle-path"),
  idlePreview: document.getElementById("idle-preview"),
  talkingPath: document.getElementById("talking-path"),
  talkingPreview: document.getElementById("talking-preview"),
  btnIdle: document.getElementById("btn-idle"),
  btnTalking: document.getElementById("btn-talking"),
  micSelect: document.getElementById("mic-select"),
  sensitivitySlider: document.getElementById("sensitivity-slider"),
  sensitivityValue: document.getElementById("sensitivity-value"),
  vuMeter: document.getElementById("vu-meter"),
  vuPeakMarker: document.getElementById("vu-peak-marker"),
  btnCalibrate: document.getElementById("btn-calibrate"),
  btnLaunch: document.getElementById("btn-launch"),
  btnHide: document.getElementById("btn-hide"),
  alwaysOnTop: document.getElementById("always-on-top"),
  noiseGateSlider: document.getElementById("noise-gate-slider"),
  noiseGateValue: document.getElementById("noise-gate-value"),
  holdTimeSlider: document.getElementById("hold-time-slider"),
  holdTimeValue: document.getElementById("hold-time-value"),
  opacitySlider: document.getElementById("opacity-slider"),
  opacityValue: document.getElementById("opacity-value"),
  rotationSlider: document.getElementById("rotation-slider"),
  rotationValue: document.getElementById("rotation-value"),
  lockPosition: document.getElementById("lock-position"),
  clickThrough: document.getElementById("click-through"),
  flipHorizontal: document.getElementById("flip-horizontal"),
  shadowEnabled: document.getElementById("shadow-enabled"),
  outlineEnabled: document.getElementById("outline-enabled"),
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
};

let currentSettings = null;

// --- VU meter (signature element): a row of tiles that light up with mic
// level in real time, with one tile marked as the sensitivity threshold so
// tuning the slider has instant, legible visual feedback. ---
const VU_TILE_COUNT = 24;
let vuThresholdIndex = Math.round((35 / 100) * (VU_TILE_COUNT - 1));

function buildVuMeter() {
  el.vuMeter.innerHTML = "";
  for (let i = 0; i < VU_TILE_COUNT; i++) {
    const tile = document.createElement("div");
    tile.className = "vu-tile";
    tile.dataset.band = i < VU_TILE_COUNT * 0.5 ? "low" : i < VU_TILE_COUNT * 0.8 ? "mid" : "high";
    const fill = document.createElement("div");
    fill.className = "vu-tile__fill";
    tile.appendChild(fill);
    el.vuMeter.appendChild(tile);
  }
  updateVuThresholdMarker();
}

function updateVuThresholdMarker() {
  const tiles = el.vuMeter.children;
  for (let i = 0; i < tiles.length; i++) {
    tiles[i].classList.toggle("is-threshold", i === vuThresholdIndex);
  }
}

function setVuLevel(levelPct) {
  const litCount = Math.round((levelPct / 100) * VU_TILE_COUNT);
  const tiles = el.vuMeter.children;
  for (let i = 0; i < tiles.length; i++) {
    tiles[i].classList.toggle("is-lit", i < litCount);
  }
}

// Smooths the raw, jittery per-callback volume readings into something
// that moves like a real VU meter: fast to rise (attack), gentle to fall
// (release). Purely cosmetic — the actual Idle/Talking decision still
// happens in audio_engine.rs and is unaffected by this smoothing.
let displayLevel = 0;
const VU_ATTACK = 0.55;
const VU_RELEASE = 0.12;

// Peak-hold marker: jumps instantly to a new high, then decays slowly —
// so you can see your loudest recent moment even after the bar itself has
// dropped back down.
let peakLevel = 0;
const PEAK_DECAY_PER_TICK = 0.6;

// When set, raw (unsmoothed) volume samples are collected here instead of
// only feeding the meter — used by the auto-calibrate routine below.
let calibrationCollector = null;

function setStatus(mode, text) {
  el.status.className = `status status--${mode}`;
  el.statusText.textContent = text;
}

function applySettingsToUI(settings) {
  currentSettings = settings;

  el.idlePath.textContent = settings.idleImagePath ?? "Not set";
  el.talkingPath.textContent = settings.talkingImagePath ?? "Not set";
  if (settings.idleImagePath) {
    el.idlePreview.style.backgroundImage = `url("${convertPath(settings.idleImagePath)}")`;
  }
  if (settings.talkingImagePath) {
    el.talkingPreview.style.backgroundImage = `url("${convertPath(settings.talkingImagePath)}")`;
  }

  el.sensitivitySlider.value = settings.sensitivityThreshold;
  el.sensitivityValue.textContent = settings.sensitivityThreshold;
  vuThresholdIndex = Math.round((settings.sensitivityThreshold / 100) * (VU_TILE_COUNT - 1));
  updateVuThresholdMarker();

  el.alwaysOnTop.checked = settings.characterWindow.alwaysOnTop;

  el.noiseGateSlider.value = settings.noiseGateThreshold;
  el.noiseGateValue.textContent = settings.noiseGateThreshold;

  el.holdTimeSlider.value = settings.mouthHoldTimeMs;
  el.holdTimeValue.textContent = `${settings.mouthHoldTimeMs}ms`;

  const opacityPct = Math.round(settings.characterWindow.opacity * 100);
  el.opacitySlider.value = opacityPct;
  el.opacityValue.textContent = `${opacityPct}%`;

  el.rotationSlider.value = settings.characterWindow.rotationDeg;
  el.rotationValue.textContent = `${Math.round(settings.characterWindow.rotationDeg)}°`;

  el.lockPosition.checked = settings.characterWindow.locked;
  el.clickThrough.checked = settings.characterWindow.clickThrough;
  el.flipHorizontal.checked = settings.characterWindow.flipped;
  el.shadowEnabled.checked = settings.characterWindow.shadowEnabled;
  el.outlineEnabled.checked = settings.characterWindow.outlineEnabled;

  if (settings.microphoneDeviceId) {
    el.micSelect.value = settings.microphoneDeviceId;
  }
}

function applyProfilesToUI(payload) {
  el.profileSelect.innerHTML = "";
  for (const name of payload.profiles) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    el.profileSelect.appendChild(opt);
  }
  el.profileSelect.value = payload.activeProfile;
  // Never allow deleting the last remaining profile.
  el.btnDeleteProfile.disabled = payload.profiles.length <= 1;
  el.btnDeleteProfile.style.opacity = el.btnDeleteProfile.disabled ? "0.4" : "1";
}

// Tauri asset paths need conversion to be usable in <img>/background-image.
// convertFileSrc is imported lazily to keep this file readable top-to-bottom.
function convertPath(path) {
  // eslint-disable-next-line no-undef
  return window.__TAURI__.core.convertFileSrc(path);
}

async function loadMicrophones() {
  const mics = await invoke("list_microphones");
  el.micSelect.innerHTML = "";
  if (mics.length === 0) {
    const opt = document.createElement("option");
    opt.textContent = "No microphone detected.";
    opt.value = "";
    el.micSelect.appendChild(opt);
    setStatus("error", "No microphone detected.");
    return;
  }
  for (const mic of mics) {
    const opt = document.createElement("option");
    opt.value = mic.id;
    opt.textContent = mic.name;
    el.micSelect.appendChild(opt);
  }
  if (currentSettings?.microphoneDeviceId) {
    el.micSelect.value = currentSettings.microphoneDeviceId;
  }
}

function canLaunch() {
  return (
    currentSettings?.idleImagePath &&
    currentSettings?.talkingImagePath &&
    el.micSelect.value
  );
}

function refreshLaunchButtonState() {
  el.btnLaunch.disabled = !canLaunch();
  el.btnLaunch.style.opacity = canLaunch() ? "1" : "0.5";
}

// --- Wiring: user actions -> backend commands (contract table C.3) ---

el.btnIdle.addEventListener("click", async () => {
  try {
    await invoke("select_idle_image");
  } catch (e) {
    setStatus("error", e);
  }
});

el.btnTalking.addEventListener("click", async () => {
  try {
    await invoke("select_talking_image");
  } catch (e) {
    setStatus("error", e);
  }
});

el.micSelect.addEventListener("change", async () => {
  if (el.micSelect.value) {
    await invoke("set_microphone", { id: el.micSelect.value });
    refreshLaunchButtonState();
  }
});

el.sensitivitySlider.addEventListener("input", async () => {
  const value = Number(el.sensitivitySlider.value);
  el.sensitivityValue.textContent = value;
  vuThresholdIndex = Math.round((value / 100) * (VU_TILE_COUNT - 1));
  updateVuThresholdMarker();
  await invoke("set_sensitivity", { value });
});

el.btnLaunch.addEventListener("click", async () => {
  if (!canLaunch()) {
    if (!currentSettings?.idleImagePath) return setStatus("error", "Please choose an idle image.");
    if (!currentSettings?.talkingImagePath) return setStatus("error", "Please choose a talking image.");
    if (!el.micSelect.value) return setStatus("error", "No microphone detected.");
    return;
  }
  try {
    await invoke("launch_character");
  } catch (e) {
    setStatus("error", String(e));
  }
});

el.btnHide.addEventListener("click", async () => {
  await invoke("hide_character");
});

el.alwaysOnTop.addEventListener("change", async () => {
  await invoke("set_always_on_top", { value: el.alwaysOnTop.checked });
});

// --- v1.2 Phase 1/2: advanced controls ---

el.noiseGateSlider.addEventListener("input", async () => {
  const value = Number(el.noiseGateSlider.value);
  el.noiseGateValue.textContent = value;
  await invoke("set_noise_gate", { value });
});

el.holdTimeSlider.addEventListener("input", async () => {
  const value = Number(el.holdTimeSlider.value);
  el.holdTimeValue.textContent = `${value}ms`;
  await invoke("set_hold_time", { value });
});

el.opacitySlider.addEventListener("input", async () => {
  const pct = Number(el.opacitySlider.value);
  el.opacityValue.textContent = `${pct}%`;
  await invoke("set_character_opacity", { value: pct / 100 });
});

el.rotationSlider.addEventListener("input", async () => {
  const value = Number(el.rotationSlider.value);
  el.rotationValue.textContent = `${value}°`;
  await invoke("set_rotation", { value });
});

el.lockPosition.addEventListener("change", async () => {
  try {
    await invoke("set_locked", { value: el.lockPosition.checked });
  } catch (e) {
    setStatus("error", String(e));
  }
});

el.clickThrough.addEventListener("change", async () => {
  try {
    await invoke("set_click_through", { value: el.clickThrough.checked });
  } catch (e) {
    setStatus("error", String(e));
  }
});

el.flipHorizontal.addEventListener("change", async () => {
  await invoke("set_flipped", { value: el.flipHorizontal.checked });
});

el.shadowEnabled.addEventListener("change", async () => {
  await invoke("set_shadow_enabled", { value: el.shadowEnabled.checked });
});

el.outlineEnabled.addEventListener("change", async () => {
  await invoke("set_outline_enabled", { value: el.outlineEnabled.checked });
});

// Character scaling hotkeys: Ctrl+= grows, Ctrl+- shrinks.
document.addEventListener("keydown", async (e) => {
  if (!e.ctrlKey && !e.metaKey) return;
  if (e.key === "=" || e.key === "+") {
    e.preventDefault();
    await invoke("nudge_character_size", { grow: true });
  } else if (e.key === "-" || e.key === "_") {
    e.preventDefault();
    await invoke("nudge_character_size", { grow: false });
  }
});

// --- v1.2 Phase 4: profiles ---

el.profileSelect.addEventListener("change", async () => {
  await invoke("switch_profile", { name: el.profileSelect.value });
});

el.btnNewProfile.addEventListener("click", async () => {
  const name = window.prompt("New profile name:");
  if (name && name.trim()) {
    await invoke("create_profile", { name: name.trim() });
  }
});

el.btnDeleteProfile.addEventListener("click", async () => {
  const name = el.profileSelect.value;
  if (!name) return;
  const confirmed = window.confirm(`Delete profile "${name}"? This can't be undone.`);
  if (confirmed) {
    await invoke("delete_profile", { name });
  }
});

// --- v1.2 Phase 1: drag-and-drop image loading ---
// Tauri's native drag-drop hands over real filesystem paths (unlike a
// plain browser drop, which only gives opaque File blobs with no path) —
// see set_image_from_dropped_path in commands.rs.

function isPointInRect(x, y, rect) {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

async function handleDroppedPaths(paths, clientX, clientY) {
  const pngPaths = paths.filter((p) => p.toLowerCase().endsWith(".png"));
  if (pngPaths.length === 0) {
    setStatus("error", "Only PNG files are supported.");
    return;
  }
  const path = pngPaths[0];
  const idleRect = el.idlePreview.closest(".picker-row").getBoundingClientRect();
  const talkingRect = el.talkingPreview.closest(".picker-row").getBoundingClientRect();

  let isIdle;
  if (isPointInRect(clientX, clientY, talkingRect)) {
    isIdle = false;
  } else if (isPointInRect(clientX, clientY, idleRect)) {
    isIdle = true;
  } else {
    // Dropped elsewhere in the window: fill whichever slot is still empty,
    // defaulting to idle if both are already set.
    isIdle = !currentSettings?.idleImagePath || !!currentSettings?.talkingImagePath;
  }

  try {
    await invoke("set_image_from_dropped_path", { path, isIdle });
  } catch (e) {
    setStatus("error", String(e));
  }
}

(async function setupDragDrop() {
  try {
    const webview = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
    await webview.onDragDropEvent((event) => {
      if (event.payload.type === "drop") {
        document.body.classList.remove("drag-over");
        const scale = window.devicePixelRatio || 1;
        const x = event.payload.position.x / scale;
        const y = event.payload.position.y / scale;
        handleDroppedPaths(event.payload.paths, x, y);
      } else if (event.payload.type === "over") {
        document.body.classList.add("drag-over");
      } else {
        document.body.classList.remove("drag-over");
      }
    });
  } catch (e) {
    console.error("Drag-and-drop setup failed:", e);
  }
})();

// --- Auto-calibrate: listens to your room's silence, then your normal
// speaking voice, and picks a sensible threshold between the two. ---

function collectRawSamples(durationMs) {
  return new Promise((resolve) => {
    calibrationCollector = [];
    setTimeout(() => {
      const samples = calibrationCollector;
      calibrationCollector = null;
      resolve(samples);
    }, durationMs);
  });
}

function percentile(samples, p) {
  if (samples.length === 0) return 0;
  const sorted = [...samples].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor(p * sorted.length));
  return sorted[idx];
}

el.btnCalibrate.addEventListener("click", async () => {
  el.btnCalibrate.disabled = true;
  el.btnCalibrate.textContent = "Calibrating…";
  try {
    setStatus("idle", "Calibrating — stay quiet for a moment…");
    const noiseSamples = await collectRawSamples(1500);

    setStatus("idle", "Now talk normally, like you would while streaming…");
    const speechSamples = await collectRawSamples(2500);

    if (noiseSamples.length < 5 || speechSamples.length < 5) {
      setStatus("error", "No mic signal detected — check your microphone and try again.");
      return;
    }

    // 90th percentile of "quiet" guards against one stray click/cough
    // skewing the floor too low. 60th percentile of "speaking" represents
    // your typical loud syllables, ignoring natural pauses between words.
    const noiseCeiling = percentile(noiseSamples, 0.9);
    const speechTypical = percentile(speechSamples, 0.6);

    let threshold = noiseCeiling + (speechTypical - noiseCeiling) * 0.35;
    threshold = Math.max(noiseCeiling + 3, Math.min(speechTypical - 3, threshold));
    threshold = Math.round(Math.max(1, Math.min(99, threshold)));

    await invoke("set_sensitivity", { value: threshold });
    setStatus("listening", `● Calibrated — threshold set to ${threshold}`);
  } catch (e) {
    setStatus("error", "Calibration failed — try again.");
  } finally {
    el.btnCalibrate.disabled = false;
    el.btnCalibrate.textContent = "Auto-calibrate";
  }
});

// --- Wiring: backend events -> UI (contract table C.3) ---

listen("settings-updated", (event) => {
  applySettingsToUI(event.payload);
  refreshLaunchButtonState();
});

listen("profiles-updated", (event) => {
  applyProfilesToUI(event.payload);
});

listen("state-changed", (event) => {
  const isTalking = event.payload.state === "talking";
  setStatus("listening", isTalking ? "● Talking" : "● Listening");
});

listen("volume-level", (event) => {
  const raw = event.payload.level;

  if (calibrationCollector) {
    calibrationCollector.push(raw);
  }

  const factor = raw > displayLevel ? VU_ATTACK : VU_RELEASE;
  displayLevel = displayLevel + (raw - displayLevel) * factor;
  setVuLevel(displayLevel);

  if (displayLevel > peakLevel) {
    peakLevel = displayLevel;
  } else {
    peakLevel = Math.max(0, peakLevel - PEAK_DECAY_PER_TICK);
  }
  el.vuPeakMarker.style.left = `${peakLevel}%`;
});

listen("device-error", (event) => {
  const reason = event.payload.reason;
  if (reason === "no-device") {
    setStatus("error", "No microphone detected.");
  } else {
    setStatus("error", "Microphone disconnected.");
  }
  refreshLaunchButtonState();
});

// --- Initial load ---

(async function init() {
  setStatus("idle", "Starting up…");
  buildVuMeter();
  const settings = await invoke("get_settings");
  applySettingsToUI(settings);
  const profiles = await invoke("list_profiles");
  applyProfilesToUI(profiles);
  await loadMicrophones();
  refreshLaunchButtonState();
  setStatus("listening", "● Listening");
})();
