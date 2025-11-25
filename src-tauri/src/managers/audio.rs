use crate::audio_toolkit::{
    audio::FrameResampler,
    list_input_devices, vad::SmoothedVad, AudioRecorder, SileroVad,
    SystemAudioCapture,
};

#[cfg(target_os = "macos")]
use crate::audio_toolkit::MacOSSystemAudio;

#[cfg(target_os = "windows")]
use crate::audio_toolkit::WindowsSystemAudio;
use crate::helpers::clamshell;
use crate::settings::{get_settings, AppSettings, AudioSource};
use crate::utils;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Emitter, Manager};

fn set_mute(mute: bool) {
    // Expected behavior:
    // - Windows: works on most systems using standard audio drivers.
    // - Linux: works on many systems (PipeWire, PulseAudio, ALSA),
    //   but some distros may lack the tools used.
    // - macOS: works on most standard setups via AppleScript.
    // If unsupported, fails silently.

    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::{
                Media::Audio::{
                    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                    MMDeviceEnumerator,
                },
                System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
            };

            macro_rules! unwrap_or_return {
                ($expr:expr) => {
                    match $expr {
                        Ok(val) => val,
                        Err(_) => return,
                    }
                };
            }

            // Initialize the COM library for this thread.
            // If already initialized (e.g., by another library like Tauri), this does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let all_devices: IMMDeviceEnumerator =
                unwrap_or_return!(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL));
            let default_device =
                unwrap_or_return!(all_devices.GetDefaultAudioEndpoint(eRender, eMultimedia));
            let volume_interface = unwrap_or_return!(
                default_device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            );

            let _ = volume_interface.SetMute(mute, std::ptr::null());
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let mute_val = if mute { "1" } else { "0" };
        let amixer_state = if mute { "mute" } else { "unmute" };

        // Try multiple backends to increase compatibility
        // 1. PipeWire (wpctl)
        if Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 2. PulseAudio (pactl)
        if Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 3. ALSA (amixer)
        let _ = Command::new("amixer")
            .args(["set", "Master", amixer_state])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "set volume output muted {}",
            if mute { "true" } else { "false" }
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }
}

const WHISPER_SAMPLE_RATE: usize = 16000;

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone, Debug)]
pub enum RecordingState {
    Idle,
    Recording { binding_id: String },
}

#[derive(Clone, Debug)]
pub enum MicrophoneMode {
    AlwaysOn,
    OnDemand,
}

/* ──────────────────────────────────────────────────────────────── */

fn create_audio_recorder(
    vad_path: &str,
    app_handle: &tauri::AppHandle,
) -> Result<AudioRecorder, anyhow::Error> {
    let silero = SileroVad::new(vad_path, 0.3)
        .map_err(|e| anyhow::anyhow!("Failed to create SileroVad: {}", e))?;
    let smoothed_vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);

    // Recorder with VAD plus a spectrum-level callback that forwards updates to
    // the frontend.
    let recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?
        .with_vad(Box::new(smoothed_vad))
        .with_level_callback({
            let app_handle = app_handle.clone();
            move |levels| {
                utils::emit_levels(&app_handle, &levels);
            }
        });

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone)]
pub struct AudioRecordingManager {
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,

    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    system_capture: Arc<Mutex<Option<Box<dyn SystemAudioCapture>>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    did_mute: Arc<Mutex<bool>>,
}

impl AudioRecordingManager {
    /* ---------- construction ------------------------------------------------ */

    pub fn new(app: &tauri::AppHandle) -> Result<Self, anyhow::Error> {
        let settings = get_settings(app);
        let mode = if settings.always_on_microphone {
            MicrophoneMode::AlwaysOn
        } else {
            MicrophoneMode::OnDemand
        };

        let manager = Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            mode: Arc::new(Mutex::new(mode.clone())),
            app_handle: app.clone(),

            recorder: Arc::new(Mutex::new(None)),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            system_capture: Arc::new(Mutex::new(None)),
            is_open: Arc::new(Mutex::new(false)),
            is_recording: Arc::new(Mutex::new(false)),
            did_mute: Arc::new(Mutex::new(false)),
        };

        // Always-on?  Open immediately.
        if matches!(mode, MicrophoneMode::AlwaysOn) {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    /* ---------- helper methods --------------------------------------------- */

    fn get_effective_microphone_device(&self, settings: &AppSettings) -> Option<cpal::Device> {
        // Check if we're in clamshell mode and have a clamshell microphone configured
        let use_clamshell_mic = if let Ok(is_clamshell) = clamshell::is_clamshell() {
            is_clamshell && settings.clamshell_microphone.is_some()
        } else {
            false
        };

        let device_name = if use_clamshell_mic {
            settings.clamshell_microphone.as_ref().unwrap()
        } else {
            settings.selected_microphone.as_ref()?
        };

        // Find the device by name
        match list_input_devices() {
            Ok(devices) => devices
                .into_iter()
                .find(|d| d.name == *device_name)
                .map(|d| d.device),
            Err(e) => {
                debug!("Failed to list devices, using default: {}", e);
                None
            }
        }
    }

    /* ---------- microphone life-cycle -------------------------------------- */

    /// Applies mute if mute_while_recording is enabled and stream is open
    pub fn apply_mute(&self) {
        let settings = get_settings(&self.app_handle);
        let mut did_mute_guard = self.did_mute.lock().unwrap();

        if settings.mute_while_recording && *self.is_open.lock().unwrap() {
            set_mute(true);
            *did_mute_guard = true;
            debug!("Mute applied");
        }
    }

    /// Removes mute if it was applied
    pub fn remove_mute(&self) {
        let mut did_mute_guard = self.did_mute.lock().unwrap();
        if *did_mute_guard {
            set_mute(false);
            *did_mute_guard = false;
            debug!("Mute removed");
        }
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let mut open_flag = self.is_open.lock().unwrap();
        if *open_flag {
            debug!("Microphone stream already active");
            // Even if already open, ensure auto-transcription is started for SystemAudio
            let settings = get_settings(&self.app_handle);
            if settings.always_on_microphone {
                let audio_source = settings.audio_source.unwrap_or(AudioSource::Microphone);
                if audio_source == AudioSource::SystemAudio {
                    let is_recording = *self.is_recording.lock().unwrap();
                    if !is_recording {
                        info!("🔄 [AudioSource] Stream already open, ensuring auto-transcription starts...");
                        let binding_id = "transcribe".to_string();
                        if self.try_start_recording(&binding_id) {
                            info!("✅ [AudioSource] Auto-transcription started for already-open stream");
                            // Note: Auto-transcription thread should be running from initial start
                            // If it's not, we need to spawn it - but we can't easily check if it's running
                            // So we rely on the fact that it should have been spawned when stream first opened
                        }
                    }
                }
            }
            return Ok(());
        }

        let start_time = Instant::now();
        let settings = get_settings(&self.app_handle);
        let audio_source = settings.audio_source.unwrap_or(AudioSource::Microphone);

        // Don't mute immediately - caller will handle muting after audio feedback
        let mut did_mute_guard = self.did_mute.lock().unwrap();
        *did_mute_guard = false;

        if audio_source == AudioSource::SystemAudio {
            // System Audio Capture - macOS
            #[cfg(target_os = "macos")]
            {
                info!("Initializing system audio capture (macOS)");
                let mut capture = MacOSSystemAudio::new(&self.app_handle)?;
                match capture.start_capture() {
                    Ok(()) => {
                        *self.system_capture.lock().unwrap() = Some(Box::new(capture));
                        *open_flag = true;
                        info!(
                            "System audio capture initialized in {:?}",
                            start_time.elapsed()
                        );
                    },
                    Err(e) => {
                        error!("Failed to start system audio capture: {}", e);
                        *open_flag = false;
                        // Don't set system_capture if start failed
                        return Err(e);
                    }
                }
                
                // Auto-start recording in always-on mode with system audio
                let settings = get_settings(&self.app_handle);
                if settings.always_on_microphone {
                    info!("Always-on mode: Auto-starting continuous system audio transcription");
                    let binding_id = "transcribe".to_string();
                    if self.try_start_recording(&binding_id) {
                        info!("Auto-started recording in always-on mode");
                        
                        // Start continuous transcription loop with sliding window (no audio loss like Google Translate)
                        let app_handle = self.app_handle.clone();
                        let rm = Arc::new(self.clone());
                        std::thread::spawn(move || {
                            use std::time::Duration;
                            use std::collections::VecDeque;
                            
                            const TRANSCRIBE_INTERVAL_SECS: u64 = 3; // Transcribe every 3 seconds for real-time
                            const MIN_AUDIO_SECS: usize = 2; // Minimum 2 seconds of audio before transcribing
                            const OVERLAP_SECS: usize = 1; // Keep 1 second overlap to avoid missing audio
                            const MIN_SAMPLES: usize = MIN_AUDIO_SECS * 16000;
                            const OVERLAP_SAMPLES: usize = OVERLAP_SECS * 16000;
                            
                            // System audio from SCK is 48kHz, need to resample to 16kHz for Whisper
                            const SYSTEM_AUDIO_SAMPLE_RATE: usize = 48000;
                            const TARGET_SAMPLE_RATE: usize = 16000;
                            let mut resampler = FrameResampler::new(
                                SYSTEM_AUDIO_SAMPLE_RATE,
                                TARGET_SAMPLE_RATE,
                                Duration::from_millis(30),
                            );
                            
                            // Accumulation buffer to avoid missing any audio (stores resampled 16kHz samples)
                            let mut accumulated_buffer: VecDeque<f32> = VecDeque::new();
                            
                            // Track previous RMS to detect when audio starts (transitions from silence to non-silence)
                            let mut previous_rms: Option<f32> = None;
                            let mut silence_detected_count = 0u64;
                            
                            info!("Auto-transcription thread started, interval: {}s (real-time mode, no audio loss)", TRANSCRIBE_INTERVAL_SECS);
                            info!("📊 [Auto-transcription] Resampler initialized: {}kHz -> {}kHz", SYSTEM_AUDIO_SAMPLE_RATE, TARGET_SAMPLE_RATE);
                            let _ = app_handle.emit("log-update", format!("✅ [Auto-transcription] Thread started - waiting for audio samples..."));
                            
                            loop {
                                std::thread::sleep(Duration::from_secs(TRANSCRIBE_INTERVAL_SECS));
                                
                                // Check if still in always-on mode
                                let settings = crate::settings::get_settings(&app_handle);
                                if !settings.always_on_microphone {
                                    info!("Always-on mode disabled, stopping auto-transcription");
                                    break;
                                }
                                
                                // Check if audio source is still SystemAudio (may have changed)
                                let audio_source = settings.audio_source.unwrap_or(crate::settings::AudioSource::Microphone);
                                if audio_source != crate::settings::AudioSource::SystemAudio {
                                    info!("Audio source changed from SystemAudio to {:?}, stopping auto-transcription", audio_source);
                                    break;
                                }
                                
                                // Ensure recording is active (for system audio, this just ensures buffer is ready)
                                if !*rm.is_recording.lock().unwrap() {
                                    if !rm.try_start_recording(&binding_id) {
                                        warn!("Failed to restart recording in always-on mode");
                                        break;
                                    }
                                }
                                
                                // Read new samples from system capture buffer and add to accumulation buffer
                                let new_samples = {
                                    #[cfg(any(target_os = "macos", target_os = "windows"))]
                                    {
                                        if let Some(capture) = rm.system_capture.lock().unwrap().as_mut() {
                                                match capture.read_samples() {
                                                Ok(Some(s)) => {
                                                    if !s.is_empty() {
                                                        info!("🎙️ [Auto-transcription] ✅ Read {} new samples from system capture ({}s audio)", s.len(), s.len() / 16000);
                                                        // Don't emit log-update for every read - too frequent, causes UI lag
                                                        // Only log to backend, frontend doesn't need to know every read
                                                        Some(s)
                                                    } else {
                                                        debug!("Auto-transcription: System capture returned empty samples");
                                                        None
                                                    }
                                                },
                                                Ok(None) => {
                                                    // Buffer is empty - this is normal if no audio is playing
                                                    // Log periodically to show we're checking
                                                    static EMPTY_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                                                    let count = EMPTY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                                    if count % 10 == 0 {
                                                        info!("🔍 [Auto-transcription] System capture buffer is empty (checked {} times) - SCStream may not be sending audio buffers", count + 1);
                                                        let _ = app_handle.emit("log-update", format!("🔍 [Auto-transcription] Buffer empty (checked {} times) - Please ensure audio is playing from Chrome or another app", count + 1));
                                                    }
                                                    None
                                                },
                                                Err(e) => {
                                                    error!("❌ [Auto-transcription] Failed to read samples from system capture: {}", e);
                                                    let _ = app_handle.emit("log-update", format!("❌ [Auto-transcription] Failed to read samples: {}", e));
                                                    None
                                                }
                                            }
                                        } else {
                                            warn!("⚠️ [Auto-transcription] System capture not available");
                                            let _ = app_handle.emit("log-update", "⚠️ [Auto-transcription] System capture not available");
                                            None
                                        }
                                    }
                                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                                    {
                                        None
                                    }
                                };
                                
                                // Resample and add new samples to accumulation buffer
                                if let Some(new_samples) = new_samples {
                                    let input_count = new_samples.len();
                                    
                                    // Resample from 48kHz to 16kHz
                                    let mut resampled_samples = Vec::new();
                                    resampler.push(&new_samples, |chunk| {
                                        resampled_samples.extend_from_slice(chunk);
                                    });
                                    
                                    let resampled_count = resampled_samples.len();
                                    accumulated_buffer.extend(resampled_samples);
                                    let total_count = accumulated_buffer.len();
                                    
                                    info!("📥 [Auto-transcription] Resampled {} samples (48kHz) -> {} samples (16kHz), total buffer: {} samples ({}s)", 
                                        input_count,
                                        resampled_count,
                                        total_count, 
                                        total_count / 16000);
                                    
                                    // Don't emit log-update for resampling - too frequent, causes UI lag
                                    // Only log to backend
                                } else {
                                    // Log periodically when no samples are available
                                    static NO_SAMPLES_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                                    let count = NO_SAMPLES_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    if count % 20 == 0 {
                                        warn!("Auto-transcription: No audio samples available (checked {} times). Please check Screen Recording permission!", count + 1);
                                        let _ = app_handle.emit("log-update", format!("⚠️ [Auto-transcription] No audio samples available (checked {} times)", count + 1));
                                    }
                                }
                                
                                // Only transcribe if we have enough audio (minimum 2 seconds)
                                let current_buffer_size = accumulated_buffer.len();
                                if current_buffer_size >= MIN_SAMPLES {
                                    info!("✅ [Auto-transcription] Buffer has {} samples ({}s), MIN_SAMPLES={}, ready to transcribe!", 
                                        current_buffer_size, 
                                        current_buffer_size / 16000,
                                        MIN_SAMPLES);
                                    let _ = app_handle.emit("log-update", format!("🔄 [Auto-transcription] Buffer ready: {}s audio, starting transcription...", current_buffer_size / 16000));
                                    // Take samples for transcription (keep overlap for next iteration)
                                    let samples_to_transcribe: Vec<f32> = if accumulated_buffer.len() > OVERLAP_SAMPLES {
                                        // Take all except overlap samples
                                        let take_count = accumulated_buffer.len() - OVERLAP_SAMPLES;
                                        accumulated_buffer.drain(..take_count).collect()
                                    } else {
                                        // Not enough for overlap, take all
                                        accumulated_buffer.drain(..).collect()
                                    };
                                    
                                        if !samples_to_transcribe.is_empty() {
                                            // Calculate RMS level to check if audio has actual sound
                                            let rms = (samples_to_transcribe.iter()
                                                .map(|&s| s * s)
                                                .sum::<f32>() / samples_to_transcribe.len() as f32)
                                                .sqrt();
                                            let max_amplitude = samples_to_transcribe.iter()
                                                .map(|&s| s.abs())
                                                .fold(0.0f32, |a, b| a.max(b));
                                            
                                            info!("🎙️ [Auto-transcription] Processing {} samples ({}s audio, {}s overlap kept) - RMS: {:.6}, Max: {:.6}",
                                                samples_to_transcribe.len(),
                                                samples_to_transcribe.len() / 16000,
                                                accumulated_buffer.len() / 16000,
                                                rms,
                                                max_amplitude);
                                            
                                            // Detect transition from silence to non-silence
                                            let was_silent = previous_rms.map(|pr| pr < 0.00001).unwrap_or(true);
                                            let is_now_audio = rms > 0.00001;
                                            
                                            if was_silent && is_now_audio {
                                                info!("🎉 [Auto-transcription] ✅✅✅ AUDIO DETECTED! Audio transitioned from silence to active! RMS: {:.6}, Max: {:.6}", rms, max_amplitude);
                                                let _ = app_handle.emit("log-update", format!("🎉 [Auto-transcription] ✅✅✅ AUDIO DETECTED! RMS: {:.6}, Max: {:.6} - Live caption will start working now!", rms, max_amplitude));
                                            }
                                            
                                            // Warn if audio seems silent
                                            if rms < 0.00001 && max_amplitude < 0.01 {
                                                silence_detected_count += 1;
                                                if silence_detected_count == 1 {
                                                    // First detection - emit clear instructions
                                                    warn!("⚠️ [Auto-transcription] Audio is SILENT (RMS: {:.6}, Max: {:.6}). BlackHole is capturing but no audio is flowing.", rms, max_amplitude);
                                                    let _ = app_handle.emit("log-update", "⚠️ [Config] Audio is SILENT! Please configure Sound Output:");
                                                    let _ = app_handle.emit("log-update", "   1. Open System Settings > Sound");
                                                    let _ = app_handle.emit("log-update", "   2. Set Output to 'BlackHole 2ch' OR create Multi-Output Device");
                                                    let _ = app_handle.emit("log-update", "   3. See HUONG_DAN_CAI_DAT_BLACKHOLE.md for details");
                                                } else if silence_detected_count % 10 == 0 {
                                                    // Periodic reminder
                                                    warn!("⚠️ [Auto-transcription] Audio still silent (checked {} times). RMS: {:.6}, Max: {:.6}", silence_detected_count, rms, max_amplitude);
                                                    let _ = app_handle.emit("log-update", format!("⚠️ [Config] Still silent ({} checks). Set Sound Output to BlackHole 2ch!", silence_detected_count));
                                                }
                                            } else {
                                                // Reset silence counter when audio is detected
                                                if silence_detected_count > 0 {
                                                    info!("🎉 [Auto-transcription] ✅✅✅ AUDIO DETECTED after {} silent checks! RMS: {:.6}, Max: {:.6}", silence_detected_count, rms, max_amplitude);
                                                    let _ = app_handle.emit("log-update", format!("🎉 [Auto-transcription] ✅✅✅ AUDIO DETECTED! RMS: {:.6} - Live caption will work now!", rms));
                                                    silence_detected_count = 0;
                                                }
                                            }
                                            
                                            // Update previous RMS for next iteration
                                            previous_rms = Some(rms);
                                        
                                        // Don't emit log-update for processing - too frequent, causes UI lag
                                        // Only log to backend
                                
                                        // Trigger transcription
                                        let tm = app_handle.state::<Arc<crate::managers::transcription::TranscriptionManager>>();
                                        let hm = app_handle.state::<Arc<crate::managers::history::HistoryManager>>();
                                        let samples_clone = samples_to_transcribe.clone();
                                    
                                        // Ensure model is loaded before transcribing
                                        tm.initiate_model_load();
                                        
                                        // Wait for model to load (with timeout)
                                        let mut wait_count = 0;
                                        const MAX_WAIT: u32 = 20; // Max 10 seconds (20 * 500ms)
                                        while !tm.is_model_loaded() && wait_count < MAX_WAIT {
                                            std::thread::sleep(Duration::from_millis(500));
                                            wait_count += 1;
                                        }
                                        
                                        if !tm.is_model_loaded() {
                                            warn!("Model still not loaded after waiting, skipping transcription");
                                            let _ = app_handle.emit("log-update", "⚠️ [Auto-transcription] Model still not loaded after waiting, skipping transcription");
                                            continue;
                                        }
                                        
                                        info!("🔄 [Auto-transcription] Starting transcription for {} samples ({}s)", 
                                            samples_to_transcribe.len(),
                                            samples_to_transcribe.len() / 16000);
                                        
                                        // Don't emit log-update for starting transcription - too frequent, causes UI lag
                                        // Only log to backend
                                        
                                        match tm.transcribe(samples_to_transcribe) {
                                            Ok(transcription) => {
                                                let trimmed = transcription.trim();
                                                info!("📝 [Auto-transcription] Raw transcription received (len={}): '{}'", transcription.len(), transcription);
                                                
                                                // Emit log for debugging - short and smart
                                                if !trimmed.is_empty() {
                                                    let _ = app_handle.emit("log-update", format!("📝 [Transcription] Result ({} chars): {}", trimmed.len(), trimmed.chars().take(50).collect::<String>()));
                                                } else {
                                                    let _ = app_handle.emit("log-update", format!("⚠️ [Transcription] Empty result (RMS: {:.6})", previous_rms.unwrap_or(0.0)));
                                                }
                                                
                                                // Always log transcription results - this is important!
                                                if !trimmed.is_empty() && trimmed.len() > 1 {
                                                    // Only process if transcription has meaningful content (more than 1 char)
                                                    info!("🎯 [Auto-transcription] Result (len={}): '{}'", trimmed.len(), trimmed);
                                                    
                                                    // Emit log event
                                                    // Don't emit log-update for result - already emitted via live-caption-update
                                                    // Only log to backend
                                                    
                                                    // Save to history (async)
                                                    let hm_clone = Arc::clone(&hm);
                                                    let transcription_clone = trimmed.to_string();
                                                    let samples_clone2 = samples_clone.clone();
                                                    tauri::async_runtime::spawn(async move {
                                                        if let Err(e) = hm_clone.save_transcription(
                                                            samples_clone2,
                                                            transcription_clone.clone(),
                                                            None,
                                                            None,
                                                        ).await {
                                                            error!("Failed to save auto-transcription to history: {}", e);
                                                        }
                                                    });
                                                    
                                                    // Emit live caption event to frontend
                                                    info!("📤 [LiveCaption] Emitting event with caption ({} chars): '{}'", trimmed.len(), trimmed);
                                                    
                                                    // Emit log for debugging - short and smart
                                                    let _ = app_handle.emit("log-update", format!("✅ [LiveCaption] Caption ({} chars): {}", trimmed.len(), trimmed.chars().take(50).collect::<String>()));
                                                    
                                                    // Don't emit log-update for every caption - too frequent, causes UI lag
                                                    // Only emit the actual caption event
                                                    if let Err(e) = app_handle.emit("live-caption-update", trimmed.to_string()) {
                                                        error!("❌ [LiveCaption] Failed to emit live-caption-update event: {}", e);
                                                        let _ = app_handle.emit("log-update", format!("❌ [LiveCaption] Failed to emit: {}", e));
                                                    } else {
                                                        info!("✅ [LiveCaption] Successfully emitted live-caption-update event");
                                                    }
                                                    
                                                    // Paste the transcription
                                                    if let Err(e) = crate::utils::paste(trimmed.to_string(), app_handle.clone()) {
                                                        error!("Failed to paste auto-transcription: {}", e);
                                                    }
                                                }
                                            }
                                           Err(e) => {
                                               error!("Auto-transcription failed: {}", e);
                                           }
                                       }
                                    }
                                }
                                // Continue loop - accumulation buffer keeps growing, no audio loss
                            }
                        });
                    }
                }
                
                return Ok(());
            }
            
            // System Audio Capture - Windows
            #[cfg(target_os = "windows")]
            {
                info!("Initializing system audio capture (Windows WASAPI)");
                let mut capture = WindowsSystemAudio::new(&self.app_handle)?;
                match capture.start_capture() {
                    Ok(()) => {
                        *self.system_capture.lock().unwrap() = Some(Box::new(capture));
                        *open_flag = true;
                        info!(
                            "System audio capture initialized in {:?}",
                            start_time.elapsed()
                        );
                    },
                    Err(e) => {
                        error!("Failed to start system audio capture: {}", e);
                        *open_flag = false;
                        return Err(e);
                    }
                }
                
                // Auto-start recording in always-on mode with system audio
                let settings = get_settings(&self.app_handle);
                if settings.always_on_microphone {
                    info!("Always-on mode: Auto-starting continuous system audio transcription");
                    let binding_id = "transcribe".to_string();
                    if self.try_start_recording(&binding_id) {
                        info!("Auto-started recording in always-on mode");
                        
                        // Start continuous transcription loop with sliding window (no audio loss like Google Translate)
                        // This is the same implementation as macOS
                        let app_handle = self.app_handle.clone();
                        let rm = Arc::new(self.clone());
                        std::thread::spawn(move || {
                            use std::time::Duration;
                            use std::collections::VecDeque;
                            use crate::audio_toolkit::audio::FrameResampler;
                            
                            const TRANSCRIBE_INTERVAL_SECS: u64 = 3;
                            const MIN_AUDIO_SECS: usize = 2;
                            const OVERLAP_SECS: usize = 1;
                            const MIN_SAMPLES: usize = MIN_AUDIO_SECS * 16000;
                            const OVERLAP_SAMPLES: usize = OVERLAP_SECS * 16000;
                            const SYSTEM_AUDIO_SAMPLE_RATE: usize = 48000;
                            const TARGET_SAMPLE_RATE: usize = 16000;
                            
                            let mut resampler = FrameResampler::new(
                                SYSTEM_AUDIO_SAMPLE_RATE,
                                TARGET_SAMPLE_RATE,
                                Duration::from_millis(30),
                            );
                            
                            let mut accumulated_buffer: VecDeque<f32> = VecDeque::new();
                            let mut previous_rms: Option<f32> = None;
                            let mut silence_detected_count = 0u64;
                            
                            info!("Windows auto-transcription thread started, interval: {}s", TRANSCRIBE_INTERVAL_SECS);
                            info!("📊 [Auto-transcription] Resampler initialized: {}kHz -> {}kHz", SYSTEM_AUDIO_SAMPLE_RATE, TARGET_SAMPLE_RATE);
                            let _ = app_handle.emit("log-update", format!("✅ [Auto-transcription] Thread started - waiting for audio samples..."));
                            
                            loop {
                                std::thread::sleep(Duration::from_secs(TRANSCRIBE_INTERVAL_SECS));
                                
                                let settings = crate::settings::get_settings(&app_handle);
                                if !settings.always_on_microphone {
                                    info!("Always-on mode disabled, stopping auto-transcription");
                                    break;
                                }
                                
                                let audio_source = settings.audio_source.unwrap_or(crate::settings::AudioSource::Microphone);
                                if audio_source != crate::settings::AudioSource::SystemAudio {
                                    info!("Audio source changed from SystemAudio, stopping auto-transcription");
                                    break;
                                }
                                
                                if !*rm.is_recording.lock().unwrap() {
                                    if !rm.try_start_recording(&binding_id) {
                                        warn!("Failed to restart recording in always-on mode");
                                        break;
                                    }
                                }
                                
                                let new_samples = {
                                    if let Some(capture) = rm.system_capture.lock().unwrap().as_mut() {
                                        match capture.read_samples() {
                                            Ok(Some(s)) => {
                                                if !s.is_empty() {
                                                    info!("🎙️ [Auto-transcription] ✅ Read {} new samples from system capture ({}s audio)", s.len(), s.len() / 16000);
                                                    Some(s)
                                                } else {
                                                    debug!("Auto-transcription: System capture returned empty samples");
                                                    None
                                                }
                                            },
                                            Ok(None) => {
                                                static EMPTY_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                                                let count = EMPTY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                                if count % 10 == 0 {
                                                    info!("🔍 [Auto-transcription] System capture buffer is empty (checked {} times)", count + 1);
                                                    let _ = app_handle.emit("log-update", format!("🔍 [Auto-transcription] Buffer empty (checked {} times) - Please ensure audio is playing", count + 1));
                                                }
                                                None
                                            },
                                            Err(e) => {
                                                error!("❌ [Auto-transcription] Failed to read samples: {}", e);
                                                let _ = app_handle.emit("log-update", format!("❌ [Auto-transcription] Failed to read samples: {}", e));
                                                None
                                            }
                                        }
                                    } else {
                                        warn!("⚠️ [Auto-transcription] System capture not available");
                                        None
                                    }
                                };
                                
                                if let Some(new_samples) = new_samples {
                                    let input_count = new_samples.len();
                                    let mut resampled_samples = Vec::new();
                                    resampler.push(&new_samples, |chunk| {
                                        resampled_samples.extend_from_slice(chunk);
                                    });
                                    
                                    let resampled_count = resampled_samples.len();
                                    accumulated_buffer.extend(resampled_samples);
                                    let total_count = accumulated_buffer.len();
                                    
                                    info!("📥 [Auto-transcription] Resampled {} samples (48kHz) -> {} samples (16kHz), total buffer: {} samples ({}s)", 
                                        input_count, resampled_count, total_count, total_count / 16000);
                                } else {
                                    static NO_SAMPLES_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                                    let count = NO_SAMPLES_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    if count % 20 == 0 {
                                        warn!("Auto-transcription: No audio samples available (checked {} times)", count + 1);
                                        let _ = app_handle.emit("log-update", format!("⚠️ [Auto-transcription] No audio samples available (checked {} times)", count + 1));
                                    }
                                }
                                
                                let current_buffer_size = accumulated_buffer.len();
                                if current_buffer_size >= MIN_SAMPLES {
                                    info!("✅ [Auto-transcription] Buffer has {} samples ({}s), ready to transcribe!", 
                                        current_buffer_size, current_buffer_size / 16000);
                                    let _ = app_handle.emit("log-update", format!("🔄 [Auto-transcription] Buffer ready: {}s audio", current_buffer_size / 16000));
                                    
                                    let samples_to_transcribe: Vec<f32> = if accumulated_buffer.len() > OVERLAP_SAMPLES {
                                        let take_count = accumulated_buffer.len() - OVERLAP_SAMPLES;
                                        accumulated_buffer.drain(..take_count).collect()
                                    } else {
                                        accumulated_buffer.drain(..).collect()
                                    };
                                    
                                    if !samples_to_transcribe.is_empty() {
                                        let rms = (samples_to_transcribe.iter()
                                            .map(|&s| s * s)
                                            .sum::<f32>() / samples_to_transcribe.len() as f32)
                                            .sqrt();
                                        let max_amplitude = samples_to_transcribe.iter()
                                            .map(|&s| s.abs())
                                            .fold(0.0f32, |a, b| a.max(b));
                                        
                                        info!("🎙️ [Auto-transcription] Processing {} samples ({}s audio) - RMS: {:.6}, Max: {:.6}",
                                            samples_to_transcribe.len(), samples_to_transcribe.len() / 16000, rms, max_amplitude);
                                        
                                        let was_silent = previous_rms.map(|pr| pr < 0.00001).unwrap_or(true);
                                        let is_now_audio = rms > 0.00001;
                                        
                                        if was_silent && is_now_audio {
                                            info!("🎉 [Auto-transcription] ✅ AUDIO DETECTED! RMS: {:.6}, Max: {:.6}", rms, max_amplitude);
                                            let _ = app_handle.emit("log-update", format!("🎉 [Auto-transcription] ✅ AUDIO DETECTED! RMS: {:.6}", rms));
                                        }
                                        
                                        if rms < 0.00001 && max_amplitude < 0.01 {
                                            silence_detected_count += 1;
                                            if silence_detected_count == 1 {
                                                warn!("⚠️ [Auto-transcription] Audio is SILENT (RMS: {:.6})", rms);
                                                let _ = app_handle.emit("log-update", "⚠️ [Config] Audio is SILENT! Please play audio from Chrome/Spotify");
                                            }
                                        } else {
                                            if silence_detected_count > 0 {
                                                info!("🎉 [Auto-transcription] ✅ AUDIO DETECTED after {} silent checks!", silence_detected_count);
                                                let _ = app_handle.emit("log-update", format!("🎉 [Auto-transcription] ✅ AUDIO DETECTED! RMS: {:.6}", rms));
                                                silence_detected_count = 0;
                                            }
                                        }
                                        
                                        previous_rms = Some(rms);
                                        
                                        let tm = app_handle.state::<Arc<crate::managers::transcription::TranscriptionManager>>();
                                        let hm = app_handle.state::<Arc<crate::managers::history::HistoryManager>>();
                                        let samples_clone = samples_to_transcribe.clone();
                                        
                                        tm.initiate_model_load();
                                        
                                        let mut wait_count = 0;
                                        const MAX_WAIT: u32 = 20;
                                        while !tm.is_model_loaded() && wait_count < MAX_WAIT {
                                            std::thread::sleep(Duration::from_millis(500));
                                            wait_count += 1;
                                        }
                                        
                                        if !tm.is_model_loaded() {
                                            warn!("Model still not loaded after waiting, skipping transcription");
                                            let _ = app_handle.emit("log-update", "⚠️ [Auto-transcription] Model not loaded, skipping");
                                            continue;
                                        }
                                        
                                        info!("🔄 [Auto-transcription] Starting transcription for {} samples", samples_to_transcribe.len());
                                        
                                        match tm.transcribe(samples_to_transcribe) {
                                            Ok(transcription) => {
                                                let trimmed = transcription.trim();
                                                info!("📝 [Auto-transcription] Raw transcription (len={}): '{}'", transcription.len(), transcription);
                                                
                                                if !trimmed.is_empty() {
                                                    let _ = app_handle.emit("log-update", format!("📝 [Transcription] Result: {}", trimmed.chars().take(50).collect::<String>()));
                                                }
                                                
                                                if !trimmed.is_empty() && trimmed.len() > 1 {
                                                    info!("🎯 [Auto-transcription] Result: '{}'", trimmed);
                                                    
                                                    let hm_clone = Arc::clone(&hm);
                                                    let transcription_clone = trimmed.to_string();
                                                    let samples_clone2 = samples_clone.clone();
                                                    tauri::async_runtime::spawn(async move {
                                                        if let Err(e) = hm_clone.save_transcription(
                                                            samples_clone2,
                                                            transcription_clone.clone(),
                                                            None,
                                                            None,
                                                        ).await {
                                                            error!("Failed to save auto-transcription to history: {}", e);
                                                        }
                                                    });
                                                    
                                                    info!("📤 [LiveCaption] Emitting event with caption: '{}'", trimmed);
                                                    let _ = app_handle.emit("log-update", format!("✅ [LiveCaption] Caption: {}", trimmed.chars().take(50).collect::<String>()));
                                                    
                                                    if let Err(e) = app_handle.emit("live-caption-update", trimmed.to_string()) {
                                                        error!("❌ [LiveCaption] Failed to emit: {}", e);
                                                    } else {
                                                        info!("✅ [LiveCaption] Successfully emitted live-caption-update event");
                                                    }
                                                    
                                                    if let Err(e) = crate::utils::paste(trimmed.to_string(), app_handle.clone()) {
                                                        error!("Failed to paste auto-transcription: {}", e);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                error!("Auto-transcription failed: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
                
                return Ok(());
            }
            
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                return Err(anyhow::anyhow!("System audio capture not supported on this platform"));
            }
        }

        // Regular Microphone Capture
        let vad_path = self
            .app_handle
            .path()
            .resolve(
                "resources/models/silero_vad_v4.onnx",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| anyhow::anyhow!("Failed to resolve VAD path: {}", e))?;
        // Lazy load VAD model - only create recorder when needed to avoid blocking
        // This prevents UI lag when switching audio sources
        let mut recorder_opt = self.recorder.lock().unwrap();

        if recorder_opt.is_none() {
            info!("🔄 [AudioSource] Loading VAD model (this may take a moment)...");
            let start_vad = Instant::now();
            *recorder_opt = Some(create_audio_recorder(
                vad_path.to_str().unwrap(),
                &self.app_handle,
            )?);
            info!("✅ [AudioSource] VAD model loaded in {:?}", start_vad.elapsed());
        }

        // Get the selected device from settings, considering clamshell mode
        let selected_device = self.get_effective_microphone_device(&settings);

        if let Some(rec) = recorder_opt.as_mut() {
            rec.open(selected_device)
                .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?;
        }

        *open_flag = true;
        info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub fn stop_microphone_stream(&self) {
        let mut open_flag = self.is_open.lock().unwrap();
        if !*open_flag {
            return;
        }

        let mut did_mute_guard = self.did_mute.lock().unwrap();
        if *did_mute_guard {
            set_mute(false);
        }
        *did_mute_guard = false;

        // Stop System Capture
        #[cfg(target_os = "macos")]
        {
            if let Some(mut capture) = self.system_capture.lock().unwrap().take() {
                let _ = capture.stop_capture();
            }
        }

        if let Some(rec) = self.recorder.lock().unwrap().as_mut() {
            // If still recording, stop first.
            if *self.is_recording.lock().unwrap() {
                let _ = rec.stop();
                *self.is_recording.lock().unwrap() = false;
            }
            let _ = rec.close();
        }

        // Reset recording state to Idle so we can start recording again later
        {
            let mut state = self.state.lock().unwrap();
            *state = RecordingState::Idle;
        }

        *open_flag = false;
        debug!("Microphone stream stopped");
    }

    /* ---------- mode switching --------------------------------------------- */

    pub fn update_mode(&self, new_mode: MicrophoneMode) -> Result<(), anyhow::Error> {
        let mode_guard = self.mode.lock().unwrap();
        let cur_mode = mode_guard.clone();

        match (cur_mode, &new_mode) {
            (MicrophoneMode::AlwaysOn, MicrophoneMode::OnDemand) => {
                if matches!(*self.state.lock().unwrap(), RecordingState::Idle) {
                    drop(mode_guard);
                    self.stop_microphone_stream();
                }
            }
            (MicrophoneMode::OnDemand, MicrophoneMode::AlwaysOn) => {
                drop(mode_guard);
                self.start_microphone_stream()?;
            }
            _ => {}
        }

        *self.mode.lock().unwrap() = new_mode;
        Ok(())
    }

    /* ---------- recording --------------------------------------------------- */

    pub fn try_start_recording(&self, binding_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Idle = *state {
            // Ensure microphone is open in on-demand mode
            if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                if let Err(e) = self.start_microphone_stream() {
                    error!("Failed to open microphone stream: {e}");
                    return false;
                }
            }

            let settings = get_settings(&self.app_handle);
            let audio_source = settings.audio_source.unwrap_or(AudioSource::Microphone);

            if audio_source == AudioSource::SystemAudio {
                // System capture is continuous, so we just mark state.
                // Clear any old buffer data before starting "recording" segment.
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                {
                    if let Some(capture) = self.system_capture.lock().unwrap().as_mut() {
                        let _ = capture.read_samples(); // Clear buffer
                        *self.is_recording.lock().unwrap() = true;
                        *state = RecordingState::Recording {
                            binding_id: binding_id.to_string(),
                        };
                        debug!("System recording started for binding {binding_id}");
                        return true;
                    }
                }
                error!("System capture not available");
                return false;
            }

            // Regular microphone recording
            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                if rec.start().is_ok() {
                    *self.is_recording.lock().unwrap() = true;
                    *state = RecordingState::Recording {
                        binding_id: binding_id.to_string(),
                    };
                    debug!("Recording started for binding {binding_id}");
                    return true;
                }
            }
            error!("Recorder not available");
            false
        } else {
            false
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn get_system_audio_status(&self) -> (bool, bool) {
        // Returns (is_open, has_audio_samples)
        let is_open = *self.is_open.lock().unwrap();
        let has_audio = if is_open {
            if let Some(capture) = self.system_capture.lock().unwrap().as_mut() {
                match capture.read_samples() {
                    Ok(Some(samples)) => !samples.is_empty(),
                    Ok(None) => false,
                    Err(_) => false,
                }
            } else {
                false
            }
        } else {
            false
        };
        (is_open, has_audio)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn get_system_audio_status(&self) -> (bool, bool) {
        (false, false)
    }

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        // Prevent duplicate calls - check if we're already updating
        static IS_UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        
        if IS_UPDATING.swap(true, std::sync::atomic::Ordering::Acquire) {
            warn!("⚠️ [AudioSource] update_selected_device already in progress, skipping duplicate call");
            // Even if duplicate, wait a bit for the first call to complete
            // This handles race conditions where the first call hasn't completed yet
            // Wait longer to ensure start_microphone_stream() has time to complete
            std::thread::sleep(std::time::Duration::from_millis(500));
            
            // Check if we're in SystemAudio mode and always-on, and ensure auto-transcription is running
            let settings = get_settings(&self.app_handle);
            if settings.always_on_microphone {
                let audio_source = settings.audio_source.unwrap_or(AudioSource::Microphone);
                if audio_source == AudioSource::SystemAudio {
                    let is_open = *self.is_open.lock().unwrap();
                    let is_recording = *self.is_recording.lock().unwrap();
                    if is_open {
                        if !is_recording {
                            // System audio is open but not recording - need to start
                            info!("🔄 [AudioSource] Duplicate call detected, ensuring auto-transcription starts...");
                            let binding_id = "transcribe".to_string();
                            if self.try_start_recording(&binding_id) {
                                info!("✅ [AudioSource] Auto-transcription started after duplicate call");
                            }
                        } else {
                            info!("✅ [AudioSource] Duplicate call detected, but auto-transcription is already running");
                        }
                    } else {
                        warn!("⚠️ [AudioSource] Duplicate call detected, but stream is not open yet - first call may still be in progress");
                    }
                }
            }
            return Ok(());
        }
        
        // Use a guard to ensure IS_UPDATING is reset even on error
        struct UpdateGuard;
        impl Drop for UpdateGuard {
            fn drop(&mut self) {
                IS_UPDATING.store(false, std::sync::atomic::Ordering::Release);
            }
        }
        let _guard = UpdateGuard;
        
        // If currently open, restart the microphone stream to use the new device
        let was_open = *self.is_open.lock().unwrap();
        if was_open {
            info!("🔄 [AudioSource] Audio source changed, stopping current stream...");
            self.stop_microphone_stream();
            
            // No delay needed - stop_microphone_stream() already handles cleanup synchronously
            // The delay was causing UI lag
            
            info!("🔄 [AudioSource] Starting new stream with updated source...");
            self.start_microphone_stream()?;
            info!("✅ [AudioSource] Stream restarted successfully");
        }
        Ok(())
    }

    pub fn stop_recording(&self, binding_id: &str) -> Option<Vec<f32>> {
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                *state = RecordingState::Idle;
                drop(state);

                let settings = get_settings(&self.app_handle);
                let audio_source = settings.audio_source.unwrap_or(AudioSource::Microphone);

                let samples = if audio_source == AudioSource::SystemAudio {
                    // Read samples from system capture
                    #[cfg(target_os = "macos")]
                    {
                        if let Some(capture) = self.system_capture.lock().unwrap().as_mut() {
                            match capture.read_samples() {
                                Ok(Some(s)) => s,
                                Ok(None) => Vec::new(),
                                Err(e) => {
                                    error!("System capture read failed: {e}");
                                    Vec::new()
                                }
                            }
                        } else {
                            error!("System capture not available");
                            Vec::new()
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        Vec::new()
                    }
                } else if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    match rec.stop() {
                        Ok(buf) => buf,
                        Err(e) => {
                            error!("stop() failed: {e}");
                            Vec::new()
                        }
                    }
                } else {
                    error!("Recorder not available");
                    Vec::new()
                };

                *self.is_recording.lock().unwrap() = false;

                // In on-demand mode turn the mic off again
                if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                    self.stop_microphone_stream();
                }

                // Pad if very short
                let s_len = samples.len();
                // debug!("Got {} samples", s_len);
                if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                    let mut padded = samples;
                    padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                    Some(padded)
                } else {
                    Some(samples)
                }
            }
            _ => None,
        }
    }

    /// Cancel any ongoing recording without returning audio samples
    pub fn cancel_recording(&self) {
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Recording { .. } = *state {
            *state = RecordingState::Idle;
            drop(state);

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                let _ = rec.stop(); // Discard the result
            }

            *self.is_recording.lock().unwrap() = false;

            // In on-demand mode turn the mic off again
            if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                self.stop_microphone_stream();
            }
        }
    }
}
