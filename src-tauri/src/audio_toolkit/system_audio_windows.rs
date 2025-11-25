// Windows System Audio Capture using WASAPI Loopback
// This captures the audio that is being played by the system (e.g., Chrome, Spotify, etc.)
// Uses Windows Audio Session API (WASAPI) in loopback mode

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::audio_toolkit::system_audio::SystemAudioCapture;
use tauri::{AppHandle, Emitter};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};

/// Windows implementation using WASAPI loopback capture
pub struct WindowsSystemAudio {
    is_capturing: bool,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    app_handle: AppHandle,
    capture_thread: Option<thread::JoinHandle<()>>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
}

impl WindowsSystemAudio {
    pub fn new(app: &AppHandle) -> Result<Self> {
        Ok(Self {
            is_capturing: false,
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            app_handle: app.clone(),
            capture_thread: None,
            stop_tx: None,
        })
    }
    
    /// Find the default loopback device (what system is playing)
    /// On Windows, this is typically called "Stereo Mix" or the default output device in loopback mode
    fn find_loopback_device() -> Option<Device> {
        let host = crate::audio_toolkit::get_cpal_host();
        
        log::info!("🔍 [WindowsSystemAudio] Searching for loopback device...");
        
        // Get default output device (speakers/headphones)
        // We'll use this in loopback mode to capture what's playing
        if let Some(default_output) = host.default_output_device() {
            if let Ok(name) = default_output.name() {
                log::info!("✅ [WindowsSystemAudio] Found default output device: {}", name);
                log::info!("💡 [WindowsSystemAudio] Will use WASAPI loopback mode to capture system audio");
                return Some(default_output);
            }
        }
        
        log::warn!("⚠️ [WindowsSystemAudio] Could not find default output device");
        None
    }
    
    /// Start loopback capture from the default output device
    fn start_loopback_capture(&mut self, device: Device) -> Result<bool> {
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        log::info!("🎯 [WindowsSystemAudio] Starting loopback capture from: {}", device_name);
        
        // Get default config for the output device
        let config = device.default_output_config()
            .map_err(|e| anyhow!("Failed to get device config for {}: {}", device_name, e))?;
        
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        
        log::info!("📊 [WindowsSystemAudio] Device config: sample_rate={}Hz, channels={}, format={:?}", 
            sample_rate, channels, config.sample_format());
        
        let _ = self.app_handle.emit("log-update", format!(
            "📊 [WindowsSystemAudio] Loopback: {}, Rate: {}Hz, Channels: {}", 
            device_name, sample_rate, channels
        ));
        
        let buffer = self.sample_buffer.clone();
        let app_handle = self.app_handle.clone();
        
        // Create channel for stopping the thread
        let (tx, rx) = std::sync::mpsc::channel();
        
        // Spawn thread to run the audio stream
        let stream_handle = thread::spawn(move || {
            log::info!("🔧 [WindowsSystemAudio] Building loopback stream...");
            
            // Build input stream in loopback mode
            // Note: On Windows, we need to use the input stream API but with a loopback device
            let stream_result = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    Self::build_loopback_stream::<f32>(&device, &config, buffer.clone(), channels, app_handle.clone())
                }
                cpal::SampleFormat::I16 => {
                    Self::build_loopback_stream::<i16>(&device, &config, buffer.clone(), channels, app_handle.clone())
                }
                cpal::SampleFormat::I32 => {
                    Self::build_loopback_stream::<i32>(&device, &config, buffer.clone(), channels, app_handle.clone())
                }
                _ => {
                    log::error!("❌ [WindowsSystemAudio] Unsupported sample format: {:?}", config.sample_format());
                    return;
                }
            };
            
            match stream_result {
                Ok(stream) => {
                    log::info!("✅ [WindowsSystemAudio] Stream created successfully!");
                    let _ = app_handle.emit("log-update", "✅ [WindowsSystemAudio] Loopback stream created".to_string());
                    
                    if let Err(e) = stream.play() {
                        log::error!("❌ [WindowsSystemAudio] Failed to start stream: {}", e);
                        let _ = app_handle.emit("log-update", format!("❌ [WindowsSystemAudio] Failed to start: {}", e));
                        return;
                    }
                    
                    log::info!("✅ [WindowsSystemAudio] Stream started - capturing system audio!");
                    let _ = app_handle.emit("log-update", "✅ [WindowsSystemAudio] Capturing system audio".to_string());
                    
                    // Keep stream alive until stop signal
                    let _stream = stream;
                    let _ = rx.recv(); // Wait for stop signal
                    
                    log::info!("🛑 [WindowsSystemAudio] Stream stopped");
                }
                Err(e) => {
                    log::error!("❌ [WindowsSystemAudio] Failed to build stream: {}", e);
                    let _ = app_handle.emit("log-update", format!("❌ [WindowsSystemAudio] Failed to build stream: {}", e));
                }
            }
        });
        
        self.capture_thread = Some(stream_handle);
        self.stop_tx = Some(tx);
        self.is_capturing = true;
        
        log::info!("✅ [WindowsSystemAudio] Loopback capture started from: {}", device_name);
        
        // Wait and check for audio detection
        let mut audio_detected = false;
        let mut max_rms_seen = 0.0f32;
        
        for check_round in 1..=5 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            
            let buf = self.sample_buffer.lock().unwrap();
            let sample_count = buf.len();
            
            if sample_count > 0 {
                // Check RMS to see if audio is present
                let samples: Vec<f32> = buf.iter().rev().take(48000).cloned().collect();
                drop(buf);
                
                if !samples.is_empty() {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    let rms = (sum_sq / samples.len() as f32).sqrt();
                    let max_amp = samples.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));
                    
                    max_rms_seen = max_rms_seen.max(rms);
                    
                    log::info!("🔍 [WindowsSystemAudio] Audio check #{}: {} samples, RMS: {:.6}, Max: {:.6}", 
                        check_round, sample_count, rms, max_amp);
                    
                    if rms > 0.00001 {
                        log::info!("✅ [WindowsSystemAudio] Audio detected! RMS: {:.6}", rms);
                        let _ = self.app_handle.emit("log-update", format!(
                            "✅ [WindowsSystemAudio] Audio detected! RMS: {:.6}", rms
                        ));
                        audio_detected = true;
                        break;
                    }
                }
            } else {
                log::info!("🔍 [WindowsSystemAudio] Check #{}: No samples yet (waiting for system audio...)", 
                    check_round);
            }
        }
        
        if !audio_detected {
            log::warn!("⚠️ [WindowsSystemAudio] No audio detected after 5s");
            log::warn!("⚠️ [WindowsSystemAudio] Max RMS seen: {:.6}", max_rms_seen);
            log::warn!("💡 [WindowsSystemAudio] Please ensure audio is playing (Chrome, Spotify, etc.)");
            
            let _ = self.app_handle.emit("log-update", format!(
                "⚠️ [WindowsSystemAudio] No audio detected. Please play audio from Chrome, Spotify, etc."
            ));
        }
        
        Ok(audio_detected)
    }
    
    fn build_loopback_stream<T>(
        device: &Device,
        config: &cpal::SupportedStreamConfig,
        buffer: Arc<Mutex<VecDeque<f32>>>,
        channels: usize,
        app_handle: AppHandle,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut callback_count = 0u64;
        
        log::info!("🔧 [WindowsSystemAudio] Creating stream callback...");
        
        let stream_cb = move |data: &[T], _info: &cpal::InputCallbackInfo| {
            callback_count += 1;
            
            // Log first callback
            if callback_count == 1 {
                log::info!("🎉 [WindowsSystemAudio] First callback received! {} samples", data.len());
                let _ = app_handle.emit("log-update", format!(
                    "🎉 [WindowsSystemAudio] First callback: {} samples", data.len()
                ));
            }
            
            // Log periodically
            let should_log = callback_count <= 50 || callback_count % 100 == 0;
            
            if should_log {
                let rms = if data.is_empty() {
                    0.0
                } else {
                    let sum_sq: f32 = data.iter()
                        .map(|&s| {
                            let f: f32 = s.to_sample();
                            f * f
                        })
                        .sum();
                    (sum_sq / data.len() as f32).sqrt()
                };
                let max_amp = data.iter()
                    .map(|&s| s.to_sample::<f32>().abs())
                    .fold(0.0f32, |a, b| a.max(b));
                
                log::info!("🎵 [WindowsSystemAudio] Callback #{}: {} samples, RMS: {:.6}, Max: {:.6}", 
                    callback_count, data.len(), rms, max_amp);
                
                if callback_count <= 10 {
                    let _ = app_handle.emit("log-update", format!(
                        "🎵 [WindowsSystemAudio] Callback #{}: {} samples, RMS: {:.6}", 
                        callback_count, data.len(), rms
                    ));
                }
                
                // Warn if no audio detected
                if max_amp < 0.00001 {
                    if callback_count == 10 {
                        log::warn!("⚠️ [WindowsSystemAudio] No audio after 10 callbacks - ensure audio is playing");
                    }
                }
            }
            
            // Convert to mono and store
            let mut buf = buffer.lock().unwrap();
            
            if channels == 1 {
                buf.extend(data.iter().map(|&s| s.to_sample::<f32>()));
            } else {
                // Convert to mono by averaging channels
                for frame in data.chunks_exact(channels) {
                    let mono_sample = frame
                        .iter()
                        .map(|&s| s.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32;
                    buf.push_back(mono_sample);
                }
            }
            
            // Log buffer status periodically
            if callback_count % 1000 == 0 {
                let buf_size = buf.len();
                log::info!("📊 [WindowsSystemAudio] Buffer: {} samples ({:.1}s)", 
                    buf_size, buf_size as f32 / 48000.0);
            }
        };
        
        // Build input stream (WASAPI will handle loopback mode automatically for output devices)
        device.build_input_stream(
            &config.clone().into(),
            stream_cb,
            |err| log::error!("❌ [WindowsSystemAudio] Stream error: {}", err),
            None,
        )
    }
}

impl SystemAudioCapture for WindowsSystemAudio {
    fn start_capture(&mut self) -> Result<()> {
        if self.is_capturing {
            log::warn!("⚠️ [WindowsSystemAudio] Already capturing, stopping first...");
            let _ = self.stop_capture();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        
        log::info!("🎯 [WindowsSystemAudio] Starting WASAPI loopback capture...");
        
        // Find default output device for loopback
        if let Some(device) = Self::find_loopback_device() {
            match self.start_loopback_capture(device) {
                Ok(true) => {
                    log::info!("✅ [WindowsSystemAudio] System audio capture active (audio detected)");
                    return Ok(());
                }
                Ok(false) => {
                    log::warn!("⚠️ [WindowsSystemAudio] Capture started but no audio detected initially");
                    log::info!("💡 [WindowsSystemAudio] Will continue monitoring - play audio to start transcription");
                    return Ok(());
                }
                Err(e) => {
                    log::error!("❌ [WindowsSystemAudio] Failed to start capture: {}", e);
                    return Err(e);
                }
            }
        }
        
        Err(anyhow!("Failed to find audio output device for loopback capture"))
    }
    
    fn stop_capture(&mut self) -> Result<()> {
        if !self.is_capturing {
            return Ok(());
        }
        
        log::info!("🛑 [WindowsSystemAudio] Stopping capture...");
        
        // Signal thread to stop
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        
        // Wait for thread to finish (in background to avoid blocking)
        if let Some(thread_handle) = self.capture_thread.take() {
            std::thread::spawn(move || {
                let _ = thread_handle.join();
                log::info!("✅ [WindowsSystemAudio] Thread stopped");
            });
        }
        
        // Clear buffer
        {
            let mut buffer = self.sample_buffer.lock().unwrap();
            buffer.clear();
        }
        
        self.is_capturing = false;
        log::info!("✅ [WindowsSystemAudio] Capture stopped");
        Ok(())
    }
    
    fn read_samples(&mut self) -> Result<Option<Vec<f32>>> {
        let mut buffer = self.sample_buffer.lock().unwrap();
        
        if buffer.is_empty() {
            static CALL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 10 == 0 {
                log::debug!("🔍 [WindowsSystemAudio] Buffer empty (checked {} times)", count + 1);
            }
            return Ok(None);
        }
        
        // Drain all samples
        let sample_count = buffer.len();
        let samples: Vec<f32> = buffer.drain(..).collect();
        
        static READ_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = READ_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 100 == 0 {
            log::info!("✅ [WindowsSystemAudio] Read {} samples ({:.1}s audio)", 
                sample_count, sample_count as f32 / 48000.0);
        }
        
        Ok(Some(samples))
    }
    
    fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}
