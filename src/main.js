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
  idleFramesList: document.getElementById("idle-frames-list"),
  talkingFramesList: document.getElementById("talking-frames-list"),
  btnAddIdleFrame: document.getElementById("btn-add-idle-frame"),
  btnAddTalkingFrame: document.getElementById("btn-add-talking-frame"),
  frameIntervalSlider: document.getElementById("frame-interval-slider"),
  frameIntervalValue: document.getElementById("frame-interval-value"),
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
  emotesList: document.getElementById("emotes-list"),
  btnAddEmote: document.getElementById("btn-add-emote"),
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
  editorOverlay: document.getElementById("editor-overlay"),
  editorCanvasWrap: document.getElementById("editor-canvas-wrap"),
  editorZoomSlider: document.getElementById("editor-zoom-slider"),
  editorBtnReset: document.getElementById("editor-btn-reset"),
  editorBtnCancel: document.getElementById("editor-btn-cancel"),
  editorBtnConfirm: document.getElementById("editor-btn-confirm"),
};

let currentSettings = null;

function convertPath(path) {
  return window.__TAURI__.core.convertFileSrc(path);
}

function setStatus(mode, text) {
  el.status.className = `status status--${mode}`;
  el.statusText.textContent = text;
}

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

// --- v1.3/v1.4: multi-frame thumbnail rendering (shared by avatar + emotes) ---
// Clicking a thumbnail re-opens the editor on that exact frame (replacing
// it in place); the small "x" removes it instead, without opening the editor.

function renderFrameThumbs(container, paths, onRemove, onEdit) {
  container.innerHTML = "";
  paths.forEach((path, index) => {
    const thumb = document.createElement("div");
    thumb.className = "frame-thumb";
    thumb.style.backgroundImage = `url("${convertPath(path)}")`;
    thumb.title = "Click to re-crop";
    thumb.addEventListener("click", () => onEdit(index, path));
    const removeBtn = document.createElement("button");
    removeBtn.className = "frame-thumb__remove";
    removeBtn.type = "button";
    removeBtn.textContent = "×";
    removeBtn.title = "Remove";
    removeBtn.addEventListener("click", (e) => {
      e.stopPropagation(); // don't also trigger the thumbnail's re-crop click
      onRemove(index);
    });
    thumb.appendChild(removeBtn);
    container.appendChild(thumb);
  });
}
//
// Every frame in the app (avatar idle/talking, and emote frames) is forced
// through this editor before it's saved. It always exports at a fixed
// EDITOR_SIZE x EDITOR_SIZE resolution regardless of the source image's
// original size/aspect ratio — this is what guarantees perfect alignment
// when frames cycle: every frame, from every state, from every emote, is
// pixel-dimension-identical. All cropping/positioning math happens here in
// JS via Canvas; the Rust side only ever writes final bytes to disk
// (save_processed_frame in commands.rs) so there's no duplicate image-math
// to keep in sync between two languages.

const EDITOR_SIZE = 512;
const baseCanvas = document.getElementById("editor-base-canvas");
const baseCtx = baseCanvas.getContext("2d");
const gridCanvas = document.getElementById("editor-grid-canvas");
const gridCtx = gridCanvas.getContext("2d");

let editorImage = null;
let editorOffsetX = 0;
let editorOffsetY = 0;
let editorScale = 1;
let editorBaseScale = 1;
let editorTarget = null; // { kind: 'idle'|'talking'|'emote', id?, replaceIndex? }
let editorDragging = false;
let editorLastX = 0;
let editorLastY = 0;

function drawEditorGrid() {
  gridCtx.clearRect(0, 0, EDITOR_SIZE, EDITOR_SIZE);
  gridCtx.strokeStyle = "rgba(255,255,255,0.3)";
  gridCtx.lineWidth = 1;
  const step = EDITOR_SIZE / 4;
  for (let i = 1; i < 4; i++) {
    gridCtx.beginPath();
    gridCtx.moveTo(i * step, 0);
    gridCtx.lineTo(i * step, EDITOR_SIZE);
    gridCtx.moveTo(0, i * step);
    gridCtx.lineTo(EDITOR_SIZE, i * step);
    gridCtx.stroke();
  }
  gridCtx.strokeStyle = "rgba(255,122,89,0.55)";
  gridCtx.beginPath();
  gridCtx.moveTo(EDITOR_SIZE / 2, 0);
  gridCtx.lineTo(EDITOR_SIZE / 2, EDITOR_SIZE);
  gridCtx.moveTo(0, EDITOR_SIZE / 2);
  gridCtx.lineTo(EDITOR_SIZE, EDITOR_SIZE / 2);
  gridCtx.stroke();
}

function redrawEditorBase() {
  baseCtx.clearRect(0, 0, EDITOR_SIZE, EDITOR_SIZE);
  if (!editorImage) return;
  const w = editorImage.naturalWidth * editorScale;
  const h = editorImage.naturalHeight * editorScale;
  baseCtx.drawImage(editorImage, editorOffsetX, editorOffsetY, w, h);
}

/// Default view: "cover" fit (fills the whole frame with no empty space,
/// cropping whatever overflows) centered — a sensible starting point most
/// images won't need much adjustment from.
function fitEditorImage() {
  if (!editorImage) return;
  editorBaseScale = Math.max(
    EDITOR_SIZE / editorImage.naturalWidth,
    EDITOR_SIZE / editorImage.naturalHeight
  );
  editorScale = editorBaseScale;
  editorOffsetX = (EDITOR_SIZE - editorImage.naturalWidth * editorScale) / 2;
  editorOffsetY = (EDITOR_SIZE - editorImage.naturalHeight * editorScale) / 2;
  el.editorZoomSlider.value = 100;
  redrawEditorBase();
}

async function openEditor(path, target) {
  editorTarget = target;
  try {
    const dataUrl = await invoke("read_image_as_data_url", { path });
    editorImage = new Image();
    editorImage.onload = fitEditorImage;
    editorImage.src = dataUrl;
    el.editorOverlay.classList.remove("hidden");
    drawEditorGrid();
  } catch (e) {
    setStatus("error", String(e));
  }
}

function closeEditor() {
  el.editorOverlay.classList.add("hidden");
  editorImage = null;
  editorTarget = null;
}

async function dispatchEditorTarget(path) {
  const t = editorTarget;
  if (t.kind === "idle") {
    if (t.replaceIndex != null) await invoke("replace_idle_frame", { index: t.replaceIndex, path });
    else await invoke("append_idle_frame", { path });
  } else if (t.kind === "talking") {
    if (t.replaceIndex != null) await invoke("replace_talking_frame", { index: t.replaceIndex, path });
    else await invoke("append_talking_frame", { path });
  } else if (t.kind === "emote") {
    if (t.replaceIndex != null) await invoke("replace_emote_frame", { id: t.id, index: t.replaceIndex, path });
    else await invoke("append_emote_frame", { id: t.id, path });
  }
}

el.editorZoomSlider.addEventListener("input", () => {
  const pct = Number(el.editorZoomSlider.value);
  const newScale = editorBaseScale * (pct / 100);
  // Zoom around the canvas center, not the top-left corner, so the subject
  // stays roughly in place while zooming instead of drifting off-frame.
  const cx = EDITOR_SIZE / 2;
  const cy = EDITOR_SIZE / 2;
  const imgCx = (cx - editorOffsetX) / editorScale;
  const imgCy = (cy - editorOffsetY) / editorScale;
  editorScale = newScale;
  editorOffsetX = cx - imgCx * editorScale;
  editorOffsetY = cy - imgCy * editorScale;
  redrawEditorBase();
});

el.editorCanvasWrap.addEventListener("mousedown", (e) => {
  editorDragging = true;
  editorLastX = e.clientX;
  editorLastY = e.clientY;
});

window.addEventListener("mousemove", (e) => {
  if (!editorDragging) return;
  const rect = el.editorCanvasWrap.getBoundingClientRect();
  const scaleFactor = EDITOR_SIZE / rect.width; // CSS px -> canvas px
  editorOffsetX += (e.clientX - editorLastX) * scaleFactor;
  editorOffsetY += (e.clientY - editorLastY) * scaleFactor;
  editorLastX = e.clientX;
  editorLastY = e.clientY;
  redrawEditorBase();
});

window.addEventListener("mouseup", () => {
  editorDragging = false;
});

el.editorCanvasWrap.addEventListener(
  "wheel",
  (e) => {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.08 : 1 / 1.08;
    const newPct = Math.max(10, Math.min(400, Number(el.editorZoomSlider.value) * factor));
    el.editorZoomSlider.value = newPct;
    el.editorZoomSlider.dispatchEvent(new Event("input"));
  },
  { passive: false }
);

el.editorBtnReset.addEventListener("click", fitEditorImage);
el.editorBtnCancel.addEventListener("click", closeEditor);

el.editorBtnConfirm.addEventListener("click", async () => {
  const base64Png = baseCanvas.toDataURL("image/png").split(",")[1];
  try {
    const path = await invoke("save_processed_frame", { base64Png });
    await dispatchEditorTarget(path);
    closeEditor();
  } catch (e) {
    setStatus("error", String(e));
  }
});



function applySettingsToUI(settings) {
  currentSettings = settings;

  renderFrameThumbs(
    el.idleFramesList,
    settings.idleFrames || [],
    async (index) => {
      await invoke("remove_idle_frame", { index });
    },
    (index, path) => {
      openEditor(path, { kind: "idle", replaceIndex: index });
    }
  );
  renderFrameThumbs(
    el.talkingFramesList,
    settings.talkingFrames || [],
    async (index) => {
      await invoke("remove_talking_frame", { index });
    },
    (index, path) => {
      openEditor(path, { kind: "talking", replaceIndex: index });
    }
  );

  el.frameIntervalSlider.value = settings.frameIntervalMs;
  el.frameIntervalValue.textContent = `${settings.frameIntervalMs}ms`;

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

  renderEmotes(settings.emotes || []);
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
  el.btnDeleteProfile.disabled = payload.profiles.length <= 1;
  el.btnDeleteProfile.style.opacity = el.btnDeleteProfile.disabled ? "0.4" : "1";
}

// --- v1.3: emote cards ---

function renderEmotes(emotes) {
  el.emotesList.innerHTML = "";
  for (const emote of emotes) {
    el.emotesList.appendChild(buildEmoteCard(emote));
  }
}

function buildEmoteCard(emote) {
  const card = document.createElement("div");
  card.className = "emote-card";

  const header = document.createElement("div");
  header.className = "emote-card__header";

  const nameInput = document.createElement("input");
  nameInput.className = "emote-card__name";
  nameInput.value = emote.name;
  nameInput.addEventListener("change", async () => {
    await invoke("rename_emote", { id: emote.id, name: nameInput.value });
  });

  const deleteBtn = document.createElement("button");
  deleteBtn.className = "btn btn--ghost btn--icon";
  deleteBtn.type = "button";
  deleteBtn.textContent = "🗑";
  deleteBtn.title = "Delete emote";
  deleteBtn.addEventListener("click", async () => {
    if (window.confirm(`Delete emote "${emote.name}"?`)) {
      await invoke("delete_emote", { id: emote.id });
    }
  });

  header.appendChild(nameInput);
  header.appendChild(deleteBtn);
  card.appendChild(header);

  const thumbsContainer = document.createElement("div");
  thumbsContainer.className = "frame-thumbs";
  renderFrameThumbs(
    thumbsContainer,
    emote.framePaths || [],
    async (index) => {
      await invoke("remove_emote_frame", { id: emote.id, index });
    },
    (index, path) => {
      openEditor(path, { kind: "emote", id: emote.id, replaceIndex: index });
    }
  );
  card.appendChild(thumbsContainer);

  const addFrameBtn = document.createElement("button");
  addFrameBtn.className = "btn-link";
  addFrameBtn.type = "button";
  addFrameBtn.textContent = "+ Add image";
  addFrameBtn.style.marginTop = "6px";
  addFrameBtn.addEventListener("click", async () => {
    try {
      const path = await invoke("pick_image_file");
      openEditor(path, { kind: "emote", id: emote.id });
    } catch (e) {
      setStatus("error", String(e));
    }
  });
  card.appendChild(addFrameBtn);

  const durationRow = document.createElement("div");
  durationRow.className = "emote-card__row";
  const durationLabel = document.createElement("span");
  durationLabel.textContent = `${emote.durationMs}ms`;
  const durationSlider = document.createElement("input");
  durationSlider.type = "range";
  durationSlider.className = "slider";
  durationSlider.min = "200";
  durationSlider.max = "10000";
  durationSlider.step = "100";
  durationSlider.value = emote.durationMs;
  durationSlider.addEventListener("input", async () => {
    const value = Number(durationSlider.value);
    durationLabel.textContent = `${value}ms`;
    await invoke("set_emote_duration", { id: emote.id, durationMs: value });
  });
  durationRow.appendChild(durationSlider);
  durationRow.appendChild(durationLabel);
  card.appendChild(durationRow);

  const hotkeyRow = document.createElement("div");
  hotkeyRow.className = "emote-card__row";
  const hotkeyLabel = document.createElement("span");
  hotkeyLabel.textContent = "Alt+";
  const hotkeySelect = document.createElement("select");
  hotkeySelect.className = "emote-card__hotkey";
  const noneOpt = document.createElement("option");
  noneOpt.value = "";
  noneOpt.textContent = "None";
  hotkeySelect.appendChild(noneOpt);
  for (let i = 1; i <= 9; i++) {
    const opt = document.createElement("option");
    opt.value = String(i);
    opt.textContent = String(i);
    hotkeySelect.appendChild(opt);
  }
  hotkeySelect.value = emote.hotkeyDigit != null ? String(emote.hotkeyDigit) : "";
  hotkeySelect.addEventListener("change", async () => {
    const digit = hotkeySelect.value === "" ? null : Number(hotkeySelect.value);
    await invoke("set_emote_hotkey", { id: emote.id, hotkeyDigit: digit });
  });
  hotkeyRow.appendChild(hotkeyLabel);
  hotkeyRow.appendChild(hotkeySelect);
  card.appendChild(hotkeyRow);

  const actions = document.createElement("div");
  actions.className = "emote-card__actions";
  const testBtn = document.createElement("button");
  testBtn.className = "btn btn--primary";
  testBtn.type = "button";
  testBtn.textContent = "Test";
  testBtn.addEventListener("click", async () => {
    try {
      await invoke("trigger_emote", { id: emote.id });
    } catch (e) {
      setStatus("error", String(e));
    }
  });
  actions.appendChild(testBtn);
  card.appendChild(actions);

  return card;
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
    (currentSettings?.idleFrames?.length ?? 0) > 0 &&
    (currentSettings?.talkingFrames?.length ?? 0) > 0 &&
    el.micSelect.value
  );
}

function refreshLaunchButtonState() {
  el.btnLaunch.disabled = !canLaunch();
  el.btnLaunch.style.opacity = canLaunch() ? "1" : "0.5";
}

// --- Wiring: user actions -> backend commands (contract table C.3) ---

el.btnAddIdleFrame.addEventListener("click", async () => {
  try {
    const path = await invoke("pick_image_file");
    openEditor(path, { kind: "idle" });
  } catch (e) {
    setStatus("error", String(e));
  }
});

el.btnAddTalkingFrame.addEventListener("click", async () => {
  try {
    const path = await invoke("pick_image_file");
    openEditor(path, { kind: "talking" });
  } catch (e) {
    setStatus("error", String(e));
  }
});

el.frameIntervalSlider.addEventListener("input", async () => {
  const value = Number(el.frameIntervalSlider.value);
  el.frameIntervalValue.textContent = `${value}ms`;
  await invoke("set_frame_interval", { value });
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
    if (!(currentSettings?.idleFrames?.length > 0)) return setStatus("error", "Please add at least one idle frame.");
    if (!(currentSettings?.talkingFrames?.length > 0)) return setStatus("error", "Please add at least one talking frame.");
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

// --- v1.3: emotes ---

el.btnAddEmote.addEventListener("click", async () => {
  await invoke("add_emote");
});

// --- Keyboard shortcuts ---
// v1.5: moved to system-wide global hotkeys (registered in main.rs via
// tauri-plugin-global-shortcut) so Ctrl+=/-/Arrow and Alt+digit work even
// while OBS or a game has focus, not just this window. The local DOM
// keydown handler that used to live here was removed — keeping both would
// have double-fired every action while this window happened to have focus.

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
  const idleRect = el.idleFramesList.closest(".frame-group").getBoundingClientRect();
  const talkingRect = el.talkingFramesList.closest(".frame-group").getBoundingClientRect();

  let isIdle;
  if (isPointInRect(clientX, clientY, talkingRect)) {
    isIdle = false;
  } else if (isPointInRect(clientX, clientY, idleRect)) {
    isIdle = true;
  } else {
    const idleCount = currentSettings?.idleFrames?.length ?? 0;
    const talkingCount = currentSettings?.talkingFrames?.length ?? 0;
    isIdle = idleCount <= talkingCount;
  }

  try {
    const validPath = await invoke("validate_image_path", { path });
    openEditor(validPath, { kind: isIdle ? "idle" : "talking" });
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

// --- Auto-calibrate ---

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

// FIX: on startup, Tauri's webview can finish loading and start calling
// invoke() before Rust's setup() has finished registering app state
// (app.manage() runs concurrently with the webview loading, not strictly
// before it). If that race is lost, the very first invoke() call throws
// "state not managed" and every future correctly-timed retry would have
// succeeded fine. Rather than depend on winning a timing race, retry a
// few times with a short delay — this makes startup robust regardless of
// how slow setup() ever becomes in the future, not just today's cause.
async function invokeWithRetry(command, args, attempts = 10, delayMs = 100) {
  for (let i = 0; i < attempts; i++) {
    try {
      return await invoke(command, args);
    } catch (e) {
      const isStateRace = typeof e === "string" && e.includes("state not managed");
      if (!isStateRace || i === attempts - 1) throw e;
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
  }
}

(async function init() {
  setStatus("idle", "Starting up…");
  buildVuMeter();
  const settings = await invokeWithRetry("get_settings");
  applySettingsToUI(settings);
  const profiles = await invokeWithRetry("list_profiles");
  applyProfilesToUI(profiles);
  await loadMicrophones();
  refreshLaunchButtonState();
  setStatus("listening", "● Listening");
})();
