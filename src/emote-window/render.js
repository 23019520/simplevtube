// render.js — Emote Window.
//
// Same "dumb renderer" pattern as the Character Window (Architecture C.5):
// this file makes zero decisions about WHEN an emote fires — that's the
// Control Window's job via trigger_emote. Once it receives an
// emote-triggered event, though, THIS file owns all playback timing
// (frame cycling + auto-hide), since that's purely a rendering concern
// with nowhere else sensible to live — the backend's job ends at "here are
// the frames and a duration," not "manage a JS-side setTimeout chain."

const { listen } = window.__TAURI__.event;
const { invoke } = window.__TAURI__.core;

const sprite = document.getElementById("emote-sprite");

let activeTimers = [];

// v1.4 FIX: re-apply centering + click-through from here rather than only
// relying on the one-time call during Rust's setup(), which can race with
// the window actually being fully realized by the OS. This script running
// at all guarantees the window exists, so this call is always safe.
invoke("finalize_emote_window").catch((e) => console.error("finalize_emote_window failed:", e));

function clearActiveTimers() {
  for (const t of activeTimers) clearTimeout(t);
  activeTimers = [];
}

function toAssetUrl(path) {
  return window.__TAURI__.core.convertFileSrc(path);
}

listen("emote-triggered", (event) => {
  const { framePaths, durationMs } = event.payload;
  if (!framePaths || framePaths.length === 0) return;

  // A new trigger always interrupts whatever's currently playing, rather
  // than queueing — simpler mental model: "the last thing you pressed is
  // what shows," matching how a single emote-wheel slot would behave.
  clearActiveTimers();

  const urls = framePaths.map(toAssetUrl);
  const frameInterval = Math.max(50, durationMs / urls.length);

  sprite.src = urls[0];
  sprite.classList.add("visible");

  urls.forEach((url, i) => {
    if (i === 0) return; // already shown above
    const t = setTimeout(() => {
      sprite.src = url;
    }, frameInterval * i);
    activeTimers.push(t);
  });

  const hideTimer = setTimeout(() => {
    sprite.classList.remove("visible");
  }, durationMs);
  activeTimers.push(hideTimer);
});

// v1.6: reposition mode — the Control Window toggles this so you can
// drag/resize where emotes pop up. While active, the window stops being
// click-through (see set_emote_reposition_mode in commands.rs) and this
// script shows a placeholder box (via the "reposition-mode" body class in
// index.html) plus a drag region, so there's always something visible and
// grabbable even if no emote is currently playing.
listen("emote-reposition-mode", (event) => {
  const { enabled } = event.payload;
  if (enabled) {
    document.body.classList.add("reposition-mode");
    document.body.setAttribute("data-tauri-drag-region", "");
  } else {
    document.body.classList.remove("reposition-mode");
    document.body.removeAttribute("data-tauri-drag-region");
  }
});
