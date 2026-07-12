// audio_engine.rs
//
// Owns everything to do with the microphone: device enumeration, opening a
// stream, computing rolling volume, and turning that into Idle/Talking state
// via a threshold + hold-time debounce (SRS FR-002, FR-003).
//
// IMPORTANT DESIGN NOTE (fixed after first compile attempt):
// cpal::Stream is NOT Send/Sync on any platform (it wraps raw OS handles).
// Tauri requires anything placed in app.manage() state to be Send + Sync.
// The fix: the Stream never leaves the thread that created it. AudioEngine
// itself only holds thread-safe handles (Arc<AtomicU8/U32> + mpsc::Sender)
// and spawns one dedicated background thread that owns the Stream for its
// entire lifetime, controlled via commands sent over a channel.
//
// v1.2 additions (Phase 1 of the v2 roadmap):
//   - Noise gate: volume below a floor is forced to zero before it ever
//     reaches the threshold comparison or the UI meter.
//   - Adjustable mouth hold time: the debounce window is now a live
//     setting instead of a hardcoded constant.
//   - Audio smoothing: an attack/release exponential moving average is now
//     applied to the level BEFORE the Idle/Talking decision (not just
//     cosmetically in the frontend meter), reducing chatter from a raw,
//     jittery per-buffer RMS reading.

// v1.12 addition:
//   - Automatic Gain Control (AGC): a slow-following peak envelope tracks
//     how loud this mic TENDS to get, and a dynamic gain multiplier scales
//     the signal so that peak lands near a fixed target — meaning a quiet
//     mic and a loud mic both end up in a similar 0-100 range, and your
//     sensitivity threshold means roughly the same thing regardless of
//     which mic you're using. Off by default: it changes detection
//     behavior, and anyone who already tuned their threshold manually
//     shouldn't have that silently shift under them on an update.

use crate::events::{DeviceErrorEvent, DeviceErrorReason, StateChangedEvent, VoiceState, VolumeLevelEvent};
use crate::events::{EVT_DEVICE_ERROR, EVT_STATE_CHANGED, EVT_VOLUME_LEVEL};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Minimum gap between volume-level UI updates. The audio callback fires
/// far more often than any UI needs to redraw (every few ms); throttling
/// to ~20Hz keeps the meter smooth without flooding the event bus.
const VOLUME_EMIT_INTERVAL_MS: u128 = 50;

// AGC tuning: target peak of 65 (not 100) leaves headroom so a sudden
// louder-than-usual moment doesn't immediately clip at the ceiling.
// Attack is fast (envelope quickly captures "this mic can get this loud"),
// release is DELIBERATELY very slow (envelope shouldn't collapse just
// because you paused for a breath) — standard peak-follower behavior.
const AGC_TARGET_PEAK: f32 = 65.0;
const AGC_MIN_GAIN: f32 = 0.5;
const AGC_MAX_GAIN: f32 = 4.0;
const AGC_ATTACK: f32 = 0.08;
const AGC_RELEASE: f32 = 0.0008;

enum AudioCommand {
    Start(Option<String>),
    Stop,
}

/// Public-facing handle. Everything here is Send + Sync: Arc<AtomicU8/U32>
/// and mpsc::Sender<T> where T: Send both are, so AudioEngine satisfies
/// Tauri's state requirements without any unsafe impls.
pub struct AudioEngine {
    threshold: Arc<AtomicU8>,
    noise_gate: Arc<AtomicU8>,
    hold_time_ms: Arc<AtomicU32>,
    agc_enabled: Arc<AtomicBool>,
    command_tx: Sender<AudioCommand>,
}

impl AudioEngine {
    pub fn new(
        app: AppHandle,
        initial_threshold: u8,
        initial_noise_gate: u8,
        initial_hold_time_ms: u32,
        initial_agc_enabled: bool,
    ) -> Self {
        let threshold = Arc::new(AtomicU8::new(initial_threshold));
        let noise_gate = Arc::new(AtomicU8::new(initial_noise_gate));
        let hold_time_ms = Arc::new(AtomicU32::new(initial_hold_time_ms));
        let agc_enabled = Arc::new(AtomicBool::new(initial_agc_enabled));
        let (command_tx, command_rx) = mpsc::channel::<AudioCommand>();

        let thread_threshold = threshold.clone();
        let thread_noise_gate = noise_gate.clone();
        let thread_hold_time_ms = hold_time_ms.clone();
        let thread_agc_enabled = agc_enabled.clone();
        thread::spawn(move || {
            audio_control_loop(
                command_rx,
                thread_threshold,
                thread_noise_gate,
                thread_hold_time_ms,
                thread_agc_enabled,
                app,
            );
        });

        Self {
            threshold,
            noise_gate,
            hold_time_ms,
            agc_enabled,
            command_tx,
        }
    }

    /// Lists available input devices as (id, human-readable name) pairs.
    /// "id" here is the device name itself since cpal doesn't expose a
    /// stable numeric ID — this is what settings_manager persists.
    /// Safe to call from any thread; doesn't touch the live stream.
    pub fn list_input_devices() -> Vec<(String, String)> {
        let host = cpal::default_host();
        match host.input_devices() {
            Ok(devices) => devices
                .filter_map(|d| d.name().ok().map(|n| (n.clone(), n)))
                .collect(),
            Err(_) => vec![],
        }
    }

    pub fn set_threshold(&self, value: u8) {
        self.threshold.store(value.min(100), Ordering::Relaxed);
    }

    pub fn set_noise_gate(&self, value: u8) {
        self.noise_gate.store(value.min(100), Ordering::Relaxed);
    }

    pub fn set_hold_time_ms(&self, value: u32) {
        self.hold_time_ms.store(value, Ordering::Relaxed);
    }

    pub fn set_agc_enabled(&self, value: bool) {
        self.agc_enabled.store(value, Ordering::Relaxed);
    }

    pub fn start(&self, device_id: Option<String>) {
        let _ = self.command_tx.send(AudioCommand::Start(device_id));
    }

    pub fn stop(&self) {
        let _ = self.command_tx.send(AudioCommand::Stop);
    }
}

/// Runs for the lifetime of the app on its own thread. Owns the cpal Stream
/// exclusively — it is created, played, and dropped all on this one thread,
/// so it never has to be Send.
fn audio_control_loop(
    command_rx: mpsc::Receiver<AudioCommand>,
    threshold: Arc<AtomicU8>,
    noise_gate: Arc<AtomicU8>,
    hold_time_ms: Arc<AtomicU32>,
    agc_enabled: Arc<AtomicBool>,
    app: AppHandle,
) {
    let mut _current_stream: Option<Stream> = None;

    for command in command_rx {
        match command {
            AudioCommand::Start(device_id) => {
                // Drop any existing stream first (stops the old mic cleanly).
                _current_stream = None;
                _current_stream = build_and_play_stream(
                    &app,
                    &threshold,
                    &noise_gate,
                    &hold_time_ms,
                    &agc_enabled,
                    device_id,
                );
            }
            AudioCommand::Stop => {
                _current_stream = None; // dropping a cpal Stream stops it
            }
        }
    }
    // command_tx was dropped (app shutting down) -> loop ends -> stream drops.
}

/// Opens the given device (by name/id) or falls back to the OS default.
/// Emits `device-error` (no-device) if literally nothing is available,
/// per SRS FR-002.4 and the Error Handling table in A.4.
fn build_and_play_stream(
    app: &AppHandle,
    threshold: &Arc<AtomicU8>,
    noise_gate: &Arc<AtomicU8>,
    hold_time_ms: &Arc<AtomicU32>,
    agc_enabled: &Arc<AtomicBool>,
    device_id: Option<String>,
) -> Option<Stream> {
    let host = cpal::default_host();

    let device = device_id
        .as_ref()
        .and_then(|id| {
            host.input_devices()
                .ok()
                .and_then(|mut devices| devices.find(|d| d.name().map(|n| &n == id).unwrap_or(false)))
        })
        .or_else(|| host.default_input_device());

    let device = match device {
        Some(d) => d,
        None => {
            let _ = app.emit(
                EVT_DEVICE_ERROR,
                DeviceErrorEvent {
                    reason: DeviceErrorReason::NoDevice,
                },
            );
            return None;
        }
    };

    let config: StreamConfig = match device.default_input_config() {
        Ok(c) => c.into(),
        Err(_) => {
            let _ = app.emit(
                EVT_DEVICE_ERROR,
                DeviceErrorEvent {
                    reason: DeviceErrorReason::NoDevice,
                },
            );
            return None;
        }
    };

    let threshold = threshold.clone();
    let noise_gate = noise_gate.clone();
    let hold_time_ms = hold_time_ms.clone();
    let agc_enabled = agc_enabled.clone();
    let app_handle = app.clone();
    let err_app_handle = app.clone();

    // VAD state, local to this stream's lifetime. Plain mutable captures are
    // fine here (no Arc/Mutex needed) since cpal invokes this closure
    // serially on a single dedicated thread — nothing else touches these.
    let mut last_state = VoiceState::Idle;
    let mut last_flip = Instant::now();
    let mut last_volume_emit = Instant::now();
    let mut smoothed_level: f32 = 0.0;
    let mut agc_peak_envelope: f32 = AGC_TARGET_PEAK; // start assuming "typical" loudness, not silence

    let stream_result = device.build_input_stream(
        &config,
        move |data: &[f32], _| {
            // Rolling RMS over this callback's buffer (Architecture B.4).
            let sum_sq: f32 = data.iter().map(|s| s * s).sum();
            let rms = (sum_sq / data.len().max(1) as f32).sqrt();

            // Decibel-based scaling: human loudness perception is
            // logarithmic, so normal speech lands comfortably mid-scale
            // instead of barely registering with a naive linear multiply.
            let db = 20.0 * rms.max(1e-6).log10(); // roughly -120..0 dB
            let mut volume_pct = (((db + 60.0) / 60.0) * 100.0).clamp(0.0, 100.0);

            // v1.12: Automatic Gain Control. Runs BEFORE the noise gate and
            // smoothing so both of those operate on the already-normalized
            // signal — meaning your noise gate/threshold settings mean
            // roughly the same thing whether AGC is on or off, just scaled
            // to fit a quieter or louder mic automatically.
            if agc_enabled.load(Ordering::Relaxed) {
                let envelope_rate = if volume_pct > agc_peak_envelope {
                    AGC_ATTACK
                } else {
                    AGC_RELEASE
                };
                agc_peak_envelope += (volume_pct - agc_peak_envelope) * envelope_rate;
                let gain = (AGC_TARGET_PEAK / agc_peak_envelope.max(5.0)).clamp(AGC_MIN_GAIN, AGC_MAX_GAIN);
                volume_pct = (volume_pct * gain).clamp(0.0, 100.0);
            }

            // Noise gate: treat anything below the floor as silence. This
            // runs BEFORE smoothing so a constant low hum can't slowly drag
            // the smoothed level upward over time.
            let gate = noise_gate.load(Ordering::Relaxed) as f32;
            if volume_pct < gate {
                volume_pct = 0.0;
            }

            // Audio smoothing: fast attack (reacts quickly when you start
            // talking) but slower release (doesn't instantly snap to zero
            // on brief dips), applied to the authoritative level used for
            // both the VAD decision and the emitted meter reading — not
            // just a cosmetic frontend effect.
            let smoothing_factor = if volume_pct > smoothed_level { 0.5 } else { 0.2 };
            smoothed_level += (volume_pct - smoothed_level) * smoothing_factor;

            let threshold_val = threshold.load(Ordering::Relaxed) as f32;
            let raw_state = if smoothed_level >= threshold_val {
                VoiceState::Talking
            } else {
                VoiceState::Idle
            };

            let hold_time = hold_time_ms.load(Ordering::Relaxed) as u128;
            let should_flip = match (last_state, raw_state) {
                (VoiceState::Idle, VoiceState::Talking) => true, // talk starts immediately, no debounce needed
                (VoiceState::Talking, VoiceState::Idle) => last_flip.elapsed().as_millis() >= hold_time,
                _ => false,
            };

            if should_flip {
                last_state = raw_state;
                last_flip = Instant::now();
                let timestamp_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let _ = app_handle.emit(
                    EVT_STATE_CHANGED,
                    StateChangedEvent {
                        state: raw_state,
                        timestamp_ms,
                    },
                );
            }

            // Live meter feed, throttled to ~20Hz regardless of how often
            // this callback fires.
            if last_volume_emit.elapsed().as_millis() >= VOLUME_EMIT_INTERVAL_MS {
                last_volume_emit = Instant::now();
                let _ = app_handle.emit(EVT_VOLUME_LEVEL, VolumeLevelEvent { level: smoothed_level });
            }
        },
        move |err| {
            eprintln!("Audio stream error: {err}");
            let _ = err_app_handle.emit(
                EVT_DEVICE_ERROR,
                DeviceErrorEvent {
                    reason: DeviceErrorReason::Disconnected,
                },
            );
        },
        None,
    );

    match stream_result {
        Ok(stream) => match stream.play() {
            Ok(()) => Some(stream),
            Err(e) => {
                eprintln!("Failed to start audio stream: {e}");
                None
            }
        },
        Err(e) => {
            eprintln!("Failed to build audio stream: {e}");
            let _ = app.emit(
                EVT_DEVICE_ERROR,
                DeviceErrorEvent {
                    reason: DeviceErrorReason::NoDevice,
                },
            );
            None
        }
    }
}
