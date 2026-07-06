// render.js — Character Window.
//
// Deliberately dumb, per Architecture doc C.5: this file makes zero
// decisions about WHEN to swap or WHAT the effects should be — it only
// applies whatever settings.characterWindow says. All VAD logic lives in
// audio_engine.rs; all effect toggling lives in the Control Window's UI.
//
// Uses window.__TAURI__ global (no bundler in this project — see main.js).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const sprite = document.getElementById("sprite");

let idlePath = null;
let talkingPath = null;

function toAssetUrl(path) {
  return window.__TAURI__.core.convertFileSrc(path);
}

function applySettings(settings) {
  idlePath = settings.idleImagePath ? toAssetUrl(settings.idleImagePath) : null;
  talkingPath = settings.talkingImagePath ? toAssetUrl(settings.talkingImagePath) : null;
  if (idlePath) sprite.src = idlePath;

  const cw = settings.characterWindow;

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
  const nextSrc = isTalking ? talkingPath : idlePath;
  if (nextSrc && sprite.src !== nextSrc) {
    sprite.src = nextSrc;
  }
});

// Initial sync in case this window loads after the first broadcast.
(async function init() {
  const settings = await invoke("get_settings");
  applySettings(settings);
})();
