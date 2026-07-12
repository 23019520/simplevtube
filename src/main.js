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
  waveformCanvas: document.getElementById("waveform-canvas"),
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
  physicsEnabled: document.getElementById("physics-enabled"),
  physicsIntensitySlider: document.getElementById("physics-intensity-slider"),
  physicsIntensityValue: document.getElementById("physics-intensity-value"),
  emotesList: document.getElementById("emotes-list"),
  btnAddEmote: document.getElementById("btn-add-emote"),
  emoteRepositionMode: document.getElementById("emote-reposition-mode"),
  status: document.getElementById("status"),
  statusText: document.getElementById("status-text"),
  editorOverlay: document.getElementById("editor-overlay"),
  editorCanvasWrap: document.getElementById("editor-canvas-wrap"),
  editorZoomSlider: document.getElementById("editor-zoom-slider"),
  editorOnionToggle: document.getElementById("editor-onion-toggle"),
  editorBtnReset: document.getElementById("editor-btn-reset"),
  editorBtnCancel: document.getElementById("editor-btn-cancel"),
  editorBtnConfirm: document.getElementById("editor-btn-confirm"),
  btnOpenPalette: document.getElementById("btn-open-palette"),
  paletteOverlay: document.getElementById("palette-overlay"),
  paletteInput: document.getElementById("palette-input"),
  paletteResults: document.getElementById("palette-results"),
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

// --- v1.9: live scrolling waveform ---
// A genuine scrolling amplitude trace, not just the VU tiles' single
// current-level readout. Rides the existing volume-level event (already
// emitted at ~20Hz from audio_engine.rs) — no backend changes needed,
// this is purely a different visualization of data already flowing in.

const waveformCtx = el.waveformCanvas.getContext("2d");
const WAVEFORM_HISTORY = 120; // ~6 seconds at the ~20Hz emit rate
const waveformHistory = new Array(WAVEFORM_HISTORY).fill(0);

function pushWaveformSample(levelPct) {
  waveformHistory.push(levelPct);
  if (waveformHistory.length > WAVEFORM_HISTORY) waveformHistory.shift();
  drawWaveform();
}

function drawWaveform() {
  const w = el.waveformCanvas.width;
  const h = el.waveformCanvas.height;
  const mid = h / 2;
  waveformCtx.clearRect(0, 0, w, h);

  const step = w / (WAVEFORM_HISTORY - 1);
  const style = getComputedStyle(document.documentElement);
  const accent = style.getPropertyValue("--accent").trim() || "#ff7a59";

  waveformCtx.beginPath();
  waveformHistory.forEach((level, i) => {
    const amplitude = (level / 100) * (mid - 4);
    const x = i * step;
    const yTop = mid - amplitude;
    if (i === 0) waveformCtx.moveTo(x, yTop);
    else waveformCtx.lineTo(x, yTop);
  });
  for (let i = WAVEFORM_HISTORY - 1; i >= 0; i--) {
    const amplitude = (waveformHistory[i] / 100) * (mid - 4);
    const x = i * step;
    const yBottom = mid + amplitude;
    waveformCtx.lineTo(x, yBottom);
  }
  waveformCtx.closePath();
  waveformCtx.fillStyle = accent;
  waveformCtx.globalAlpha = 0.75;
  waveformCtx.fill();

  waveformCtx.globalAlpha = 1;
  waveformCtx.strokeStyle = "rgba(255,255,255,0.15)";
  waveformCtx.lineWidth = 1;
  waveformCtx.beginPath();
  waveformCtx.moveTo(0, mid);
  waveformCtx.lineTo(w, mid);
  waveformCtx.stroke();
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
const onionCanvas = document.getElementById("editor-onion-canvas");
const onionCtx = onionCanvas.getContext("2d");
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

// --- v1.7: onion skinning ---
// Shows the previously-added frame (and the one before it, fainter/more
// blurred) as a ghost reference beneath the frame you're currently
// positioning, so cycling animations stay visually aligned. Onion frames
// are drawn from their FINAL exported form (already 512x512, exactly what
// they'll look like in the app) — no pan/zoom applies to them, only to the
// new image you're actively working on.

const ONION_LAYERS = [
  { alpha: 0.35, blur: 2 }, // most recently added frame
  { alpha: 0.18, blur: 4 }, // the one before that
];

/// Finds up to two reference frames for onion skinning.
///
/// For idle/talking: the OPPOSITE state's most recent frame is the primary
/// reference (alpha/blur layer 0) — this is what actually matters most,
/// since idle and talking need to line up with EACH OTHER so the character
/// doesn't visibly shift position when it swaps states. This applies even
/// with just one frame in each state, unlike same-state history which only
/// exists once you have 2+ frames in one state. The same state's own
/// immediately-previous frame (if any) is a secondary, fainter reference
/// for multi-frame cycling alignment within that state.
///
/// For emotes: unrelated to idle/talking (they don't composite onto the
/// avatar), so this stays self-referencing — the emote's own previous 1-2
/// frames, same as before.
function getOnionSkinFrames(target) {
  if (target.kind === "emote") {
    const emote = (currentSettings?.emotes || []).find((e) => e.id === target.id);
    const list = emote?.framePaths || [];
    const cutoff = target.replaceIndex != null ? target.replaceIndex : list.length;
    return list.slice(Math.max(0, cutoff - 2), cutoff).reverse();
  }

  const sameList = target.kind === "idle" ? currentSettings?.idleFrames || [] : currentSettings?.talkingFrames || [];
  const oppositeList = target.kind === "idle" ? currentSettings?.talkingFrames || [] : currentSettings?.idleFrames || [];

  const refs = [];
  if (oppositeList.length > 0) {
    refs.push(oppositeList[oppositeList.length - 1]); // most recent frame of the OTHER state
  }
  const cutoff = target.replaceIndex != null ? target.replaceIndex : sameList.length;
  const samePrevious = sameList.slice(Math.max(0, cutoff - 1), cutoff)[0]; // immediately-previous same-state frame
  if (samePrevious && !refs.includes(samePrevious)) {
    refs.push(samePrevious);
  }
  return refs.slice(0, 2);
}

function drawOnionSkin(paths) {
  onionCtx.clearRect(0, 0, EDITOR_SIZE, EDITOR_SIZE);
  // Draw farthest-back reference first so the most recent frame ends up
  // visually on top where they overlap.
  [...paths].reverse().forEach((path, i) => {
    const layerIndex = paths.length - 1 - i;
    const cfg = ONION_LAYERS[layerIndex];
    if (!cfg) return;
    const img = new Image();
    img.onload = () => {
      onionCtx.save();
      onionCtx.globalAlpha = cfg.alpha;
      onionCtx.filter = `blur(${cfg.blur}px)`;
      onionCtx.drawImage(img, 0, 0, EDITOR_SIZE, EDITOR_SIZE);
      onionCtx.restore();
    };
    img.src = convertPath(path);
  });
}


let onionSkinEnabled = true; // session-level preference, sticky across editor opens
let editorOnionPaths = []; // computed fresh each time the editor opens, independent of the toggle

/// Applies (or clears) the onion skin based on the current toggle state.
/// Separated from openEditor so flipping the checkbox mid-edit updates
/// instantly without needing to recompute which frames are the reference.
function applyOnionSkinVisibility() {
  if (onionSkinEnabled && editorOnionPaths.length > 0) {
    drawOnionSkin(editorOnionPaths);
    baseCanvas.style.opacity = "0.85";
  } else {
    onionCtx.clearRect(0, 0, EDITOR_SIZE, EDITOR_SIZE);
    baseCanvas.style.opacity = "1";
  }
}

el.editorOnionToggle.addEventListener("change", () => {
  onionSkinEnabled = el.editorOnionToggle.checked;
  applyOnionSkinVisibility();
});

async function openEditor(path, target) {
  editorTarget = target;

  editorOnionPaths = getOnionSkinFrames(target);
  applyOnionSkinVisibility();

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
  editorOnionPaths = [];
  onionCtx.clearRect(0, 0, EDITOR_SIZE, EDITOR_SIZE);
  baseCanvas.style.opacity = "1";
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
  el.physicsEnabled.checked = settings.characterWindow.physicsEnabled;
  const physicsIntensityPct = Math.round(settings.characterWindow.physicsIntensity ?? 50);
  el.physicsIntensitySlider.value = physicsIntensityPct;
  el.physicsIntensityValue.textContent = `${physicsIntensityPct}%`;

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

el.physicsEnabled.addEventListener("change", async () => {
  await invoke("set_physics_enabled", { value: el.physicsEnabled.checked });
});

el.physicsIntensitySlider.addEventListener("input", async () => {
  const value = Number(el.physicsIntensitySlider.value);
  el.physicsIntensityValue.textContent = `${value}%`;
  await invoke("set_physics_intensity", { value });
});

// --- v1.3: emotes ---

el.btnAddEmote.addEventListener("click", async () => {
  await invoke("add_emote");
});

// v1.6: emote popup position/size — toggles the Emote Window between
// click-through/invisible (normal operation) and draggable/resizable with
// a visible placeholder box (reposition mode).
el.emoteRepositionMode.addEventListener("change", async () => {
  try {
    await invoke("set_emote_reposition_mode", { enabled: el.emoteRepositionMode.checked });
  } catch (e) {
    setStatus("error", String(e));
  }
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

  pushWaveformSample(raw);
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

// --- v1.8: command palette ---
//
// A single searchable list of everything actionable in the app. Static
// entries (toggle a checkbox, open a section) are hand-written below;
// dynamic entries (one per profile, one per emote) are generated fresh
// every time the palette opens, so a newly added emote or profile shows up
// immediately without this list needing to be told about it separately.

function getPaletteActions() {
  const actions = [
    {
      label: "Launch character",
      category: "Character",
      icon: "▶",
      run: () => el.btnLaunch.click(),
    },
    {
      label: "Hide character",
      category: "Character",
      icon: "⏸",
      run: () => invoke("hide_character"),
    },
    {
      label: "Auto-calibrate microphone",
      category: "Audio",
      icon: "🎚",
      run: () => el.btnCalibrate.click(),
    },
    {
      label: "Toggle always-on-top",
      category: "Character",
      icon: "📌",
      run: () => {
        el.alwaysOnTop.checked = !el.alwaysOnTop.checked;
        el.alwaysOnTop.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Toggle lock position & size",
      category: "Character",
      icon: "🔒",
      run: () => {
        el.lockPosition.checked = !el.lockPosition.checked;
        el.lockPosition.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Toggle click-through",
      category: "Character",
      icon: "👆",
      run: () => {
        el.clickThrough.checked = !el.clickThrough.checked;
        el.clickThrough.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Flip character horizontally",
      category: "Character",
      icon: "↔",
      run: () => {
        el.flipHorizontal.checked = !el.flipHorizontal.checked;
        el.flipHorizontal.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Toggle drop shadow",
      category: "Character",
      icon: "🌑",
      run: () => {
        el.shadowEnabled.checked = !el.shadowEnabled.checked;
        el.shadowEnabled.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Toggle outline",
      category: "Character",
      icon: "◽",
      run: () => {
        el.outlineEnabled.checked = !el.outlineEnabled.checked;
        el.outlineEnabled.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Toggle reactive bounce & jiggle (physics)",
      category: "Character",
      icon: "🌊",
      run: () => {
        el.physicsEnabled.checked = !el.physicsEnabled.checked;
        el.physicsEnabled.dispatchEvent(new Event("change"));
      },
    },
    {
      label: "Undo",
      category: "Edit",
      icon: "↩",
      run: () => runUndo(),
    },
    {
      label: "Redo",
      category: "Edit",
      icon: "↪",
      run: () => runRedo(),
    },
    {
      label: "New profile…",
      category: "Profiles",
      icon: "➕",
      run: () => el.btnNewProfile.click(),
    },
    {
      label: "New emote…",
      category: "Emotes",
      icon: "➕",
      run: () => el.btnAddEmote.click(),
    },
    {
      label: "Add idle frame…",
      category: "Character",
      icon: "🖼",
      run: () => el.btnAddIdleFrame.click(),
    },
    {
      label: "Add talking frame…",
      category: "Character",
      icon: "🖼",
      run: () => el.btnAddTalkingFrame.click(),
    },
    {
      label: "Toggle emote position & resize mode",
      category: "Emotes",
      icon: "🎯",
      run: () => {
        el.emoteRepositionMode.checked = !el.emoteRepositionMode.checked;
        el.emoteRepositionMode.dispatchEvent(new Event("change"));
      },
    },
  ];

  // Dynamic: one entry per profile, to switch directly to it.
  for (const name of Array.from(el.profileSelect.options).map((o) => o.value)) {
    if (!name) continue;
    actions.push({
      label: `Switch to profile: ${name}`,
      category: "Profiles",
      icon: "👤",
      run: () => invoke("switch_profile", { name }),
    });
  }

  // Dynamic: one entry per emote, to fire it directly.
  for (const emote of currentSettings?.emotes || []) {
    actions.push({
      label: `Fire emote: ${emote.name}`,
      category: "Emotes",
      icon: "✨",
      run: async () => {
        try {
          await invoke("trigger_emote", { id: emote.id });
        } catch (e) {
          setStatus("error", String(e));
        }
      },
    });
  }

  return actions;
}

/// Simple subsequence fuzzy match (like VSCode's palette): every character
/// of the query must appear in order in the target, not necessarily
/// adjacent. Returns null if no match, or a score (lower is better) plus
/// the matched index positions (for highlighting) if it does.
function fuzzyMatch(query, target) {
  if (query === "") return { score: 0, positions: [] };
  const q = query.toLowerCase();
  const t = target.toLowerCase();
  const positions = [];
  let qi = 0;
  let lastMatch = -1;
  let gapPenalty = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      if (lastMatch >= 0) gapPenalty += ti - lastMatch - 1;
      positions.push(ti);
      lastMatch = ti;
      qi++;
    }
  }
  if (qi < q.length) return null; // not all query chars matched
  return { score: gapPenalty + t.length * 0.01, positions };
}

function highlightLabel(label, positions) {
  if (positions.length === 0) return label;
  let out = "";
  let posIdx = 0;
  for (let i = 0; i < label.length; i++) {
    if (posIdx < positions.length && positions[posIdx] === i) {
      out += `<mark>${label[i]}</mark>`;
      posIdx++;
    } else {
      out += label[i];
    }
  }
  return out;
}

let paletteFiltered = [];
let paletteActiveIndex = 0;

function renderPaletteResults() {
  el.paletteResults.innerHTML = "";
  if (paletteFiltered.length === 0) {
    const empty = document.createElement("div");
    empty.className = "palette-empty";
    empty.textContent = "No matching commands.";
    el.paletteResults.appendChild(empty);
    return;
  }
  paletteFiltered.forEach((entry, i) => {
    const item = document.createElement("div");
    item.className = "palette-item" + (i === paletteActiveIndex ? " active" : "");
    item.innerHTML = `
      <span class="palette-item__icon">${entry.action.icon}</span>
      <span class="palette-item__label">${highlightLabel(entry.action.label, entry.positions)}</span>
      <span class="palette-item__category">${entry.action.category}</span>
    `;
    item.addEventListener("mouseenter", () => {
      paletteActiveIndex = i;
      renderPaletteResults();
    });
    item.addEventListener("click", () => runPaletteAction(entry.action));
    el.paletteResults.appendChild(item);
  });
}

function filterPalette(query) {
  const all = getPaletteActions();
  const matched = all
    .map((action) => {
      const m = fuzzyMatch(query, action.label);
      return m ? { action, score: m.score, positions: m.positions } : null;
    })
    .filter(Boolean)
    .sort((a, b) => a.score - b.score);
  paletteFiltered = matched;
  paletteActiveIndex = 0;
  renderPaletteResults();
}

async function runPaletteAction(action) {
  closePalette();
  try {
    await action.run();
  } catch (e) {
    setStatus("error", String(e));
  }
}

function openPalette() {
  el.paletteOverlay.classList.remove("hidden");
  el.paletteInput.value = "";
  filterPalette("");
  el.paletteInput.focus();
}

function closePalette() {
  el.paletteOverlay.classList.add("hidden");
}

el.btnOpenPalette.addEventListener("click", openPalette);

el.paletteInput.addEventListener("input", () => {
  filterPalette(el.paletteInput.value);
});

el.paletteOverlay.addEventListener("click", (e) => {
  if (e.target === el.paletteOverlay) closePalette();
});

el.paletteInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    paletteActiveIndex = Math.min(paletteActiveIndex + 1, paletteFiltered.length - 1);
    renderPaletteResults();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    paletteActiveIndex = Math.max(paletteActiveIndex - 1, 0);
    renderPaletteResults();
  } else if (e.key === "Enter") {
    e.preventDefault();
    const entry = paletteFiltered[paletteActiveIndex];
    if (entry) runPaletteAction(entry.action);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closePalette();
  }
});

// Ctrl+K (or Cmd+K on macOS) opens the palette from anywhere in this
// window. This one stays a local (not global-OS) shortcut deliberately —
// unlike the character resize/move/emote hotkeys, the palette only makes
// sense while looking at this window, so there's no reason to fight other
// apps for this key combo system-wide.
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    if (el.paletteOverlay.classList.contains("hidden")) {
      openPalette();
    } else {
      closePalette();
    }
  }
});

// --- v1.9: undo/redo ---
// Local shortcut (not global-OS), same reasoning as the palette above —
// undo only makes sense for changes made in this window.
async function runUndo() {
  try {
    await invoke("undo_settings");
    setStatus("listening", "● Undone");
  } catch (e) {
    setStatus("idle", String(e)); // "Nothing to undo." isn't really an error
  }
}

async function runRedo() {
  try {
    await invoke("redo_settings");
    setStatus("listening", "● Redone");
  } catch (e) {
    setStatus("idle", String(e));
  }
}

document.addEventListener("keydown", (e) => {
  const isTyping = document.activeElement && ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement.tagName);
  if (isTyping) return;
  if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "z") return;
  e.preventDefault();
  if (e.shiftKey) {
    runRedo();
  } else {
    runUndo();
  }
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
