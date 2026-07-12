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

// --- v1.10: spring-physics reactive jiggle ---
//
// A real (if simple) physics simulation: a single critically-ish-damped
// spring on the sprite's vertical offset, driven by two kinds of impulses —
// a strong one-time "pop" when speech starts, and small continuous kicks
// proportional to loudness while talking continues. Squash/stretch is
// derived directly from the spring's current velocity (fast movement =
// compressed/stretched), not a second independent spring, to keep this
// simple enough to reason about and tune without a real device to test on.
//
// This module owns its own requestAnimationFrame loop, started/stopped
// based on the physicsEnabled setting — it does NOT run when disabled, so
// it costs nothing (matches the app's "minimal CPU" NFR) for anyone who
// doesn't want the bounce.

const PHYSICS_STIFFNESS = 220; // spring constant — higher = snaps back faster
const PHYSICS_DAMPING = 16; // higher = settles faster, less oscillation
const PHYSICS_SQUASH_FACTOR = 0.012; // how much velocity translates to squash/stretch
const PHYSICS_MAX_STRETCH = 1.18;
const PHYSICS_MIN_SQUASH = 0.82;

let physicsEnabled = false;
let physicsIntensity = 50; // 0-100, mirrors settings.characterWindow.physicsIntensity
let physicsPosY = 0;
let physicsVelY = 0;
let physicsRafId = null;
let physicsLastTs = null;

// v1.11: idle breathing. Deliberately kept as a simple, separate sine wave
// rather than folded into the reactive spring above — the spring models
// impulse response (a "pop" and its ringdown), which is the wrong tool for
// a continuous, always-present sway. Two independent systems, additively
// combined at render time, stay easier to reason about and tune than one
// system trying to do both. Only contributes while NOT talking; the
// reactive spring takes over fully during speech.
const IDLE_BREATHE_PERIOD_MS = 3200; // one full up/down cycle
const IDLE_BREATHE_AMPLITUDE_PX = 4; // max vertical drift at full intensity
const IDLE_BREATHE_SQUASH_AMPLITUDE = 0.015; // subtle "chest rise" scale pulse

function physicsIntensityScale() {
  return physicsIntensity / 50; // 50 (default) => 1x, i.e. the tuned constants above assume "50"
}

function physicsImpulse(strength) {
  physicsVelY -= strength * physicsIntensityScale();
}

function physicsTick(ts) {
  if (!physicsEnabled) {
    physicsRafId = null;
    return;
  }
  if (physicsLastTs == null) physicsLastTs = ts;
  const dt = Math.min((ts - physicsLastTs) / 1000, 0.05); // clamp so a stalled/backgrounded tab can't produce a huge dt jump
  physicsLastTs = ts;

  // Semi-implicit (symplectic) Euler integration of a damped spring
  // pulling back toward 0: F = -k*x - c*v, a = F/m (mass = 1).
  const force = -PHYSICS_STIFFNESS * physicsPosY - PHYSICS_DAMPING * physicsVelY;
  physicsVelY += force * dt;
  physicsPosY += physicsVelY * dt;

  const speed = Math.abs(physicsVelY);
  let squashY = Math.max(PHYSICS_MIN_SQUASH, 1 - speed * PHYSICS_SQUASH_FACTOR);
  let squashX = Math.min(PHYSICS_MAX_STRETCH, 1 + speed * PHYSICS_SQUASH_FACTOR * 0.6);

  // v1.11: idle breathing — additive, not blended into the spring state
  // itself, so it can never accumulate or destabilize the reactive spring
  // (e.g. if talking starts and stops rapidly). Zero contribution while
  // talking, full sine-wave contribution while idle.
  let bounceY = physicsPosY;
  if (!currentlyTalking) {
    const intensityScale = physicsIntensityScale();
    const phase = (ts / IDLE_BREATHE_PERIOD_MS) * Math.PI * 2;
    const breathe = Math.sin(phase);
    bounceY += breathe * IDLE_BREATHE_AMPLITUDE_PX * intensityScale;
    const breatheSquash = breathe * IDLE_BREATHE_SQUASH_AMPLITUDE * intensityScale;
    squashY = Math.max(PHYSICS_MIN_SQUASH, Math.min(PHYSICS_MAX_STRETCH, squashY + breatheSquash));
    squashX = Math.max(PHYSICS_MIN_SQUASH, Math.min(PHYSICS_MAX_STRETCH, squashX - breatheSquash * 0.5));
  }

  sprite.style.setProperty("--bounce-y", `${bounceY.toFixed(2)}px`);
  sprite.style.setProperty("--squash-x", squashX.toFixed(3));
  sprite.style.setProperty("--squash-y", squashY.toFixed(3));

  physicsRafId = requestAnimationFrame(physicsTick);
}

function physicsSetEnabled(enabled) {
  const wasEnabled = physicsEnabled;
  physicsEnabled = enabled;
  if (enabled && !wasEnabled) {
    physicsLastTs = null;
    if (physicsRafId == null) physicsRafId = requestAnimationFrame(physicsTick);
  } else if (!enabled && wasEnabled) {
    if (physicsRafId != null) {
      cancelAnimationFrame(physicsRafId);
      physicsRafId = null;
    }
    // Reset to neutral so disabling mid-bounce doesn't freeze the sprite
    // in an offset/squashed position.
    physicsPosY = 0;
    physicsVelY = 0;
    sprite.style.setProperty("--bounce-y", "0px");
    sprite.style.setProperty("--squash-x", "1");
    sprite.style.setProperty("--squash-y", "1");
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

  physicsIntensity = cw.physicsIntensity ?? 50;
  physicsSetEnabled(!!cw.physicsEnabled);
}

listen("settings-updated", (event) => applySettings(event.payload));

listen("state-changed", (event) => {
  const isTalking = event.payload.state === "talking";
  if (isTalking === currentlyTalking) return; // no actual transition, ignore
  currentlyTalking = isTalking;
  startCycle(isTalking ? talkingFrames : idleFrames);

  // v1.10: "bounce on speech" — a one-time pop when talking STARTS. Talking
  // -> idle deliberately gets no impulse; letting the spring settle
  // naturally back to rest reads as "calming down," which feels right.
  if (isTalking) {
    physicsImpulse(55);
  }
});

// v1.10: continuous voice-intensity-scaled jiggle while talking — small
// kicks proportional to loudness, so a loud syllable visibly reacts more
// than a quiet one. Only applies while currently in the Talking state;
// idle never jiggles from ambient noise.
listen("volume-level", (event) => {
  if (!currentlyTalking || !physicsEnabled) return;
  const level = event.payload.level;
  physicsImpulse((level / 100) * 7);
});

// Initial sync in case this window loads after the first broadcast.
(async function init() {
  const settings = await invoke("get_settings");
  applySettings(settings);
})();
