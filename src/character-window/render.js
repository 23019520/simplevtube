// render.js — Character Window.
//
// Deliberately dumb, per Architecture doc C.5: this file makes zero
// decisions. It listens for two events and swaps an <img src>. All VAD
// logic lives in audio_engine.rs. If the character ever shows the wrong
// sprite, the bug is here (a rendering bug) — never a logic bug, because
// there is no logic here to have a bug in.
//
// Uses window.__TAURI__ global (no bundler in this project — see main.js).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const sprite = document.getElementById("sprite");

let idlePath = null;
let talkingPath = null;

function toAssetUrl(path) {
  // eslint-disable-next-line no-undef
  return window.__TAURI__.core.convertFileSrc(path);
}

function applySettings(settings) {
  idlePath = settings.idleImagePath ? toAssetUrl(settings.idleImagePath) : null;
  talkingPath = settings.talkingImagePath ? toAssetUrl(settings.talkingImagePath) : null;
  if (idlePath) sprite.src = idlePath;
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
