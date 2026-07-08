// render.js — Character Window.
//
// Deliberately dumb, per Architecture doc C.5: this file makes zero
// decisions about WHEN to swap states or WHAT the effects should be — it
// only applies whatever settings says. All VAD logic lives in
// audio_engine.rs; all effect toggling lives in the Control Window's UI.
//
// v1.3: idle/talking are no longer always a single static image — each can
// now be a list of frames that cycles on a timer while that state is
// active. This file owns the cycling TIMER (a rendering-timing concern,
// same reasoning as the Emote Window), but never decides which state is
// active — that's still purely audio_engine.rs's call via state-changed.
//
// Uses window.__TAURI__ global (no bundler in this project — see main.js).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const sprite = document.getElementById("sprite");

let idleFrames = [];
let talkingFrames = [];
let frameIntervalMs = 150;
let currentlyTalking = false;
let cycleTimer = null;
let cycleIndex = 0;

function toAssetUrl(path) {
  return window.__TAURI__.core.convertFileSrc(path);
}

function stopCycle() {
  if (cycleTimer) {
    clearInterval(cycleTimer);
    cycleTimer = null;
  }
}

/// Starts (or restarts) cycling through `frames` at frameIntervalMs. A
/// single-frame list just shows that one frame statically — no interval
/// timer is created, matching the old single-image behavior exactly.
function startCycle(frames) {
  stopCycle();
  cycleIndex = 0;
  if (frames.length === 0) return;
  sprite.src = frames[0];
  if (frames.length > 1) {
    cycleTimer = setInterval(() => {
      cycleIndex = (cycleIndex + 1) % frames.length;
      sprite.src = frames[cycleIndex];
    }, frameIntervalMs);
  }
}

function applySettings(settings) {
  idleFrames = (settings.idleFrames || []).map(toAssetUrl);
  talkingFrames = (settings.talkingFrames || []).map(toAssetUrl);
  frameIntervalMs = settings.frameIntervalMs || 150;

  // Restart whichever cycle is currently active so a newly added/removed
  // frame or a changed interval takes effect immediately, without waiting
  // for the next state-changed event.
  startCycle(currentlyTalking ? talkingFrames : idleFrames);

  const cw = settings.characterWindow;

  // Dragging is implemented via the data-tauri-drag-region attribute on
  // <body> (see index.html). When locked, remove it so "Lock position &
  // size" actually means nothing moves it, including a stray drag.
  if (cw.locked) {
    document.body.removeAttribute("data-tauri-drag-region");
  } else {
    document.body.setAttribute("data-tauri-drag-region", "");
  }

  // Opacity + flip + rotation are plain CSS custom properties (see the
  // combined transform/opacity/filter rule in index.html).
  sprite.style.setProperty("--opacity", cw.opacity);
  sprite.style.setProperty("--flip", cw.flipped ? "-1" : "1");
  sprite.style.setProperty("--rotate", `${cw.rotationDeg}deg`);

  // Shadow and outline both use CSS filter: drop-shadow, which (unlike
  // box-shadow) follows the PNG's actual alpha silhouette instead of
  // drawing a rectangle. An "outline" is faked by stacking four
  // zero-blur drop-shadows in each direction — a common CSS trick for a
  // solid-color silhouette outline with no dedicated outline-follows-alpha
  // property available.
  const filters = [];
  if (cw.outlineEnabled) {
    filters.push(
      "drop-shadow(1.5px 0 0 white)",
      "drop-shadow(-1.5px 0 0 white)",
      "drop-shadow(0 1.5px 0 white)",
      "drop-shadow(0 -1.5px 0 white)"
    );
  }
  if (cw.shadowEnabled) {
    filters.push("drop-shadow(0 6px 10px rgba(0,0,0,0.55))");
  }
  sprite.style.setProperty("--effect-filter", filters.length ? filters.join(" ") : "none");
}

listen("settings-updated", (event) => applySettings(event.payload));

listen("state-changed", (event) => {
  const isTalking = event.payload.state === "talking";
  if (isTalking === currentlyTalking) return; // no actual transition, ignore
  currentlyTalking = isTalking;
  startCycle(isTalking ? talkingFrames : idleFrames);
});

// Initial sync in case this window loads after the first broadcast.
(async function init() {
  const settings = await invoke("get_settings");
  applySettings(settings);
})();
