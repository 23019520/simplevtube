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
  btnLaunch: document.getElementById("btn-launch"),
  btnHide: document.getElementById("btn-hide"),
  alwaysOnTop: document.getElementById("always-on-top"),
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

  if (settings.microphoneDeviceId) {
    el.micSelect.value = settings.microphoneDeviceId;
  }
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

// --- Wiring: backend events -> UI (contract table C.3) ---

listen("settings-updated", (event) => {
  applySettingsToUI(event.payload);
  refreshLaunchButtonState();
});

listen("state-changed", (event) => {
  const isTalking = event.payload.state === "talking";
  setStatus("listening", isTalking ? "● Talking" : "● Listening");
});

listen("volume-level", (event) => {
  setVuLevel(event.payload.level);
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
  await loadMicrophones();
  refreshLaunchButtonState();
  setStatus("listening", "● Listening");
})();
