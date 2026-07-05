// audio_engine.rs
//
// Owns everything to do with the microphone: device enumeration, opening a
// stream, computing rolling volume, and turning that into Idle/Talking state
// via a threshold + hold-time debounce (SRS FR-002, FR-003).
//
// IMPORTANT DESIGN NOTE (fixed after first compile attempt):
// cpal::Stream is NOT Send/Sync on any platform (it wraps raw OS handles).
// Tauri requires anything placed in app.manage() state to be Send + Sync.
// The original version tried to hold the Stream directly in AudioEngine
// behind a Mutex, which does NOT make it Send/Sync — a Mutex<T> is only
// Sync if T: Send, and cpal::Stream explicitly opts out of Send.
//
// The fix: the Stream never leaves the thread that created it. AudioEngine
// itself only holds a thread-safe handle (Arc<AtomicU8> + mpsc::Sender) and
// spawns one dedicated background thread that owns the Stream for its
// entire lifetime, controlled via commands sent over a channel. This is
// also just a better design for audio code in general.

use crate::events::{DeviceErrorEvent, DeviceErrorReason, StateChangedEvent, VoiceState, VolumeLevelEvent};
use crate::events::{EVT_DEVICE_ERROR, EVT_STATE_CHANGED, EVT_VOLUME_LEVEL};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Minimum time a Talking state must persist before we allow it to flip back
/// to Idle. Prevents flicker between syllables/short pauses (SRS FR-003.3).
const HOLD_TIME_MS: u128 = 200;

/// Minimum gap between volume-level UI updates. The audio callback fires
/// far more often than any UI needs to redraw (every few ms); throttling
/// to ~20Hz keeps the meter smooth without flooding the event bus.
const VOLUME_EMIT_INTERVAL_MS: u128 = 50;

enum AudioCommand {
    Start(Option<String>),
    Stop,
}

/// Public-facing handle. Everything here is Send + Sync: Arc<AtomicU8> and
/// mpsc::Sender<T> where T: Send both are, so AudioEngine satisfies Tauri's
/// state requirements without any unsafe impls.
pub struct AudioEngine {
    threshold: Arc<AtomicU8>,
    command_tx: Sender<AudioCommand>,
}

impl AudioEngine {
    pub fn new(app: AppHandle, initial_threshold: u8) -> Self {
        let threshold = Arc::new(AtomicU8::new(initial_threshold));
        let (command_tx, command_rx) = mpsc::channel::<AudioCommand>();

        let thread_threshold = threshold.clone();
        thread::spawn(move || {
            audio_control_loop(command_rx, thread_threshold, app);
        });

        Self {
            threshold,
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
fn audio_control_loop(command_rx: mpsc::Receiver<AudioCommand>, threshold: Arc<AtomicU8>, app: AppHandle) {
    let mut current_stream: Option<Stream> = None;

    for command in command_rx {
        match command {
            AudioCommand::Start(device_id) => {
                // Drop any existing stream first (stops the old mic cleanly).
                current_stream = None;
                current_stream = build_and_play_stream(&app, &threshold, device_id);
            }
            AudioCommand::Stop => {
                current_stream = None; // dropping a cpal Stream stops it
            }
        }
    }
    // command_tx was dropped (app shutting down) -> loop ends -> stream drops.
}

/// Opens the given device (by name/id) or falls back to the OS default.
/// Emits `device-error` (no-device) if literally nothing is available,
/// per SRS FR-002.4 and the Error Handling table in A.4.
fn build_and_play_stream(app: &AppHandle, threshold: &Arc<AtomicU8>, device_id: Option<String>) -> Option<Stream> {
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
    let app_handle = app.clone();
    let err_app_handle = app.clone();

    // VAD state, local to this stream's lifetime.
    let last_state = Arc::new(Mutex::new(VoiceState::Idle));
    let last_flip = Arc::new(Mutex::new(Instant::now()));
    let last_volume_emit = Arc::new(Mutex::new(Instant::now()));

    let stream_result = device.build_input_stream(
        &config,
        move |data: &[f32], _| {
            // Rolling RMS over this callback's buffer (Architecture B.4).
            let sum_sq: f32 = data.iter().map(|s| s * s).sum();
            let rms = (sum_sq / data.len().max(1) as f32).sqrt();

            // FIX: the original linear scaling (rms * 400) rarely crossed
            // the sensitivity threshold during normal speech, since typical
            // speech RMS in a [-1,1] float stream is quite small. Human
            // perception of loudness is logarithmic, so we convert to
            // decibels and map a realistic dB range to 0-100 instead. This
            // makes normal speaking volume land comfortably mid-scale
            // rather than barely registering.
            let db = 20.0 * rms.max(1e-6).log10(); // roughly -120..0 dB
            let volume_pct = (((db + 60.0) / 60.0) * 100.0).clamp(0.0, 100.0);

            let threshold_val = threshold.load(Ordering::Relaxed) as f32;
            let raw_state = if volume_pct >= threshold_val {
                VoiceState::Talking
            } else {
                VoiceState::Idle
            };

            let mut state_guard = last_state.lock().unwrap();
            let mut flip_guard = last_flip.lock().unwrap();

            let should_flip = match (*state_guard, raw_state) {
                (VoiceState::Idle, VoiceState::Talking) => true, // talk starts immediately, no debounce needed
                (VoiceState::Talking, VoiceState::Idle) => flip_guard.elapsed().as_millis() >= HOLD_TIME_MS,
                _ => false,
            };

            if should_flip {
                *state_guard = raw_state;
                *flip_guard = Instant::now();
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
            // this callback fires (Architecture note: kept separate from
            // the debounced state-changed event so the meter feels
            // instantaneous while sprite-swapping stays flicker-free).
            let mut volume_emit_guard = last_volume_emit.lock().unwrap();
            if volume_emit_guard.elapsed().as_millis() >= VOLUME_EMIT_INTERVAL_MS {
                *volume_emit_guard = Instant::now();
                let _ = app_handle.emit(EVT_VOLUME_LEVEL, VolumeLevelEvent { level: volume_pct });
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
