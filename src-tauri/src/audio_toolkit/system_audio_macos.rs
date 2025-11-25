// macOS System Audio Capture
// Strategy 1: Try BlackHole virtual audio device (recommended - more reliable)
// Strategy 2: Fallback to ScreenCaptureKit (requires macOS 13+ and Screen Recording permission)

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::io::{BufRead, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::audio_toolkit::system_audio::SystemAudioCapture;
use crate::utils;
use tauri::{AppHandle, Emitter};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};

/// macOS implementation - tries BlackHole first, then ScreenCaptureKit
pub struct MacOSSystemAudio {
    is_capturing: bool,
    permission_denied: bool, // Track if permission was denied
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    capture_process: Option<Child>,
    app_handle: AppHandle,
    use_blackhole: bool, // Whether we're using BlackHole or ScreenCaptureKit
    blackhole_thread: Option<thread::JoinHandle<()>>, // Thread that keeps BlackHole stream alive
    blackhole_stop_tx: Option<std::sync::mpsc::Sender<()>>, // Channel to signal stop
}

impl MacOSSystemAudio {
    pub fn new(app: &AppHandle) -> Result<Self> {
        Ok(Self {
            is_capturing: false,
            permission_denied: false,
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            capture_process: None,
            app_handle: app.clone(),
            use_blackhole: false,
            blackhole_thread: None,
            blackhole_stop_tx: None,
        })
    }
    
    pub fn is_permission_denied(&self) -> bool {
        self.permission_denied
    }
    
    /// Try to find BlackHole device
    /// Also tries to find any input device that might have system audio
    fn find_blackhole_device() -> Option<Device> {
        let host = crate::audio_toolkit::get_cpal_host();
        log::info!("🔍 [SystemAudio] Enumerating input devices to find system audio source...");
        
        // Also check default input device
        if let Some(default_input) = host.default_input_device() {
            if let Ok(name) = default_input.name() {
                log::info!("🔍 [SystemAudio] Default input device: {}", name);
            }
        }
        
        // Also check output devices (for debugging)
        log::info!("🔍 [BlackHole] Enumerating output devices...");
        if let Ok(output_devices) = host.output_devices() {
            let mut output_list = Vec::new();
            for device in output_devices {
                if let Ok(name) = device.name() {
                    output_list.push(name.clone());
                    log::info!("🔍 [BlackHole] Found output device: {}", name);
                }
            }
            log::info!("📋 [BlackHole] All output devices: {:?}", output_list);
            
            // Check default output - CRITICAL: Audio must be routed here for capture to work
            if let Some(default_output) = host.default_output_device() {
                if let Ok(name) = default_output.name() {
                    log::info!("🔍 [SystemAudio] Default OUTPUT device: {}", name);
                    if !name.contains("BlackHole") && !name.contains("blackhole") && !name.contains("Multi-Output") {
                        log::error!("❌ [SystemAudio] ⚠️⚠️⚠️ CRITICAL: Default OUTPUT is NOT BlackHole or Multi-Output Device!");
                        log::error!("❌ [SystemAudio] Current output: '{}'", name);
                        log::error!("❌ [SystemAudio] Audio from Chrome/system will NOT be routed to BlackHole!");
                        log::error!("❌ [SystemAudio] This is why you're seeing RMS: 0.000000 (silence)");
                        log::warn!("💡 [SystemAudio] ⭐ RECOMMENDED SOLUTION: Create Multi-Output Device (giữ audio từ speakers + capture)");
                        log::warn!("   📋 Steps:");
                        log::warn!("   1. Mở Audio MIDI Setup (Applications > Utilities > Audio MIDI Setup)");
                        log::warn!("   2. Click '+' button ở bottom-left, chọn 'Create Multi-Output Device'");
                        log::warn!("   3. Trong Multi-Output Device:");
                        log::warn!("      ✅ Check box cho 'BlackHole 2ch'");
                        log::warn!("      ✅ Check box cho '{}' (speakers hiện tại)", name);
                        log::warn!("      ⚠️ Đảm bảo Master Device là '{}' (để volume control hoạt động)", name);
                        log::warn!("   4. System Settings > Sound > Output: Chọn Multi-Output Device vừa tạo");
                        log::warn!("   ✅ Kết quả: Audio vẫn phát từ speakers VÀ được capture bởi app!");
                        log::warn!("");
                        log::warn!("   ⚠️ Alternative (KHÔNG KHUYẾN NGHỊ): Chọn 'BlackHole 2ch' trực tiếp");
                        log::warn!("      ❌ Bạn sẽ KHÔNG nghe được âm thanh từ speakers!");
                        log::warn!("      ✅ Chỉ dùng nếu bạn không cần nghe audio (ví dụ: chỉ cần transcription)");
                    } else {
                        log::info!("✅ [SystemAudio] Default OUTPUT is '{}' - audio should be routed correctly", name);
                        if name.contains("Multi-Output") {
                            log::info!("✅ [SystemAudio] Multi-Output Device detected - audio will route to both speakers and BlackHole");
                        }
                    }
                }
            } else {
                log::warn!("⚠️ [SystemAudio] Could not get default output device");
            }
        }
        
        if let Ok(devices) = host.input_devices() {
            let mut device_list = Vec::new();
            let mut blackhole_device: Option<Device> = None;
            
            for device in devices {
                if let Ok(name) = device.name() {
                    device_list.push(name.clone());
                    log::info!("🔍 [SystemAudio] Found input device: {}", name);
                    
                    // Priority 1: BlackHole (best for system audio)
                    if name.contains("BlackHole") || name.contains("blackhole") {
                        log::info!("✅ [SystemAudio] Found BlackHole device: {}", name);
                        blackhole_device = Some(device);
                        break; // Use first BlackHole found
                    }
                }
            }
            
            log::info!("📋 [SystemAudio] All input devices: {:?}", device_list);
            
            if blackhole_device.is_some() {
                return blackhole_device;
            }
            
            // If no BlackHole found, log all devices for debugging
            log::warn!("⚠️ [SystemAudio] BlackHole device not found in input devices.");
            log::info!("💡 [SystemAudio] Available input devices: {:?}", device_list);
            log::info!("💡 [SystemAudio] To capture system audio, you need:");
            log::info!("   1. Install BlackHole: brew install blackhole-2ch");
            log::info!("   2. Configure Sound Output to route audio to BlackHole");
            log::info!("   3. Or create Multi-Output Device (BlackHole + Speakers)");
        } else {
            log::warn!("⚠️ [SystemAudio] Failed to enumerate input devices");
        }
        
        log::info!("⚠️ [SystemAudio] No suitable input device found. Will try ScreenCaptureKit.");
        None
    }
    
    /// Start capture from BlackHole device
    /// Returns true if audio is detected (RMS > threshold), false if silent
    fn start_blackhole_capture(&mut self, device: Device) -> Result<bool> {
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        log::info!("🎯 Starting capture from device: {}", device_name);
        
        let config = device.default_input_config()
            .map_err(|e| anyhow!("Failed to get device config for {}: {}", device_name, e))?;
        
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        
        log::info!("📊 Device config ({}): sample_rate={}, channels={}, format={:?}", 
            device_name, sample_rate, channels, config.sample_format());
        
        let buffer = self.sample_buffer.clone();
        
        // Create stream in thread worker (like AudioRecorder does)
        // This avoids Send issues since stream stays in the thread
        let (tx, rx) = std::sync::mpsc::channel();
        let stream_handle = thread::spawn(move || {
            // Build and start stream in this thread
            let stream_result: Result<cpal::Stream, cpal::BuildStreamError> = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    Self::build_blackhole_stream_in_thread::<f32>(&device, &config, buffer.clone(), channels)
                }
                cpal::SampleFormat::I16 => {
                    Self::build_blackhole_stream_in_thread::<i16>(&device, &config, buffer.clone(), channels)
                }
                cpal::SampleFormat::I32 => {
                    Self::build_blackhole_stream_in_thread::<i32>(&device, &config, buffer.clone(), channels)
                }
                _ => {
                    log::error!("Unsupported BlackHole sample format: {:?}", config.sample_format());
                    return; // Exit thread if unsupported format
                }
            };
            
            match stream_result {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        log::error!("Failed to start BlackHole stream: {}", e);
                        return;
                    }
                    log::info!("✅ BlackHole stream started in worker thread");
                    
                    // Keep stream alive - wait for stop signal
                    let _stream = stream; // Stream stays alive as long as this variable exists
                    let _ = rx.recv(); // Wait for stop signal
                    // Stream will be dropped here
                    log::info!("BlackHole stream stopped");
                }
                Err(e) => {
                    log::error!("Failed to build BlackHole stream: {}", e);
                }
            }
        });
        
        // Store sender to signal stop later
        self.blackhole_thread = Some(stream_handle);
        self.blackhole_stop_tx = Some(tx);
        self.use_blackhole = true;
        self.is_capturing = true;
        
        log::info!("✅ Capture started successfully from device: {}", device_name);
        
        // Wait a bit and check if audio is present
        // Check multiple times over 5 seconds to catch audio that starts later
        let mut audio_detected = false;
        let mut max_rms_seen = 0.0f32;
        let mut max_amp_seen = 0.0f32;
        
        for check_round in 1..=5 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let buf = self.sample_buffer.lock().unwrap();
            let sample_count = buf.len();
            drop(buf);
            
            if sample_count > 0 {
                // Check RMS of recent samples to see if audio is present
                let buf = self.sample_buffer.lock().unwrap();
                let samples: Vec<f32> = buf.iter().rev().take(48000).cloned().collect(); // Check last 1 second
                drop(buf);
                
                if !samples.is_empty() {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    let rms = (sum_sq / samples.len() as f32).sqrt();
                    let max_amp = samples.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));
                    
                    max_rms_seen = max_rms_seen.max(rms);
                    max_amp_seen = max_amp_seen.max(max_amp);
                    
                    log::info!("🔍 [SystemAudio] Audio check #{} after {}s: {} samples, RMS: {:.6}, Max: {:.6}", 
                        check_round, check_round, sample_count, rms, max_amp);
                    
                    if rms > 0.00001 {
                        log::info!("✅ [SystemAudio] ✅✅✅ AUDIO DETECTED! RMS: {:.6}, Max: {:.6}", rms, max_amp);
                        let _ = self.app_handle.emit("log-update", format!(
                            "✅✅✅ [SystemAudio] AUDIO DETECTED! RMS: {:.6}, Max: {:.6} - Live caption will start working now!", rms, max_amp
                        ));
                        audio_detected = true;
                        break;
                    }
                }
            } else {
                log::info!("🔍 [SystemAudio] Audio check #{} after {}s: No samples yet (waiting for audio from {}...)", 
                    check_round, check_round, device_name);
            }
        }
        
        if !audio_detected {
            log::warn!("⚠️ [SystemAudio] No audio detected after 5s from device: {}", device_name);
            log::warn!("⚠️ [SystemAudio] Max RMS seen: {:.6}, Max amplitude seen: {:.6}", max_rms_seen, max_amp_seen);
            log::warn!("⚠️ [SystemAudio] User may need to configure Sound Output to route audio to this device");
            log::warn!("⚠️ [SystemAudio] Will continue monitoring - audio may start later when user configures output");
            
            // Emit detailed log to frontend
            let _ = self.app_handle.emit("log-update", format!(
                "⚠️ [SystemAudio] No audio detected from {}. Max RMS: {:.6}. Please configure Sound Output to route audio to this device.", 
                device_name, max_rms_seen
            ));
        }
        
        Ok(audio_detected)
    }
    
    fn build_blackhole_stream_in_thread<T>(
        device: &Device,
        config: &cpal::SupportedStreamConfig,
        buffer: Arc<Mutex<VecDeque<f32>>>,
        channels: usize,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut callback_count = 0u64;
        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            callback_count += 1;
            let mut buf = buffer.lock().unwrap();
            
            // Calculate RMS for first few callbacks to check if audio is present
            if callback_count <= 10 {
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
                
                // Debug: Check first few raw samples to see what we're getting
                let first_samples: Vec<f32> = data.iter().take(10).map(|&s| s.to_sample::<f32>()).collect();
                log::info!("🎵 [BlackHole] Callback #{}: {} samples, RMS: {:.6}, Max: {:.6}, First 10: {:?}", 
                    callback_count, data.len(), rms, max_amp, first_samples);
                
                // If all samples are zero, log a warning
                if max_amp < 0.00001 && callback_count == 5 {
                    log::warn!("⚠️ [BlackHole] All samples are ZERO! This means BlackHole is not receiving audio.");
                    log::warn!("⚠️ [BlackHole] Check: 1) Is Multi-Output Device set as Sound Output? 2) Is audio actually playing?");
                }
            }
            
            if channels == 1 {
                buf.extend(data.iter().map(|&sample| sample.to_sample::<f32>()));
            } else {
                // Convert to mono
                for frame in data.chunks_exact(channels) {
                    let mono_sample = frame
                        .iter()
                        .map(|&sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32;
                    buf.push_back(mono_sample);
                }
            }
            
            // Log periodically (every 1000 callbacks = ~20 seconds at 48kHz)
            if callback_count % 1000 == 0 {
                let buf_size = buf.len();
                // Calculate RMS of recent samples for logging
                let recent_samples: Vec<f32> = buf.iter().rev().take(48000).cloned().collect(); // Last 1 second
                let rms = if !recent_samples.is_empty() {
                    let sum_sq: f32 = recent_samples.iter().map(|&s| s * s).sum();
                    (sum_sq / recent_samples.len() as f32).sqrt()
                } else {
                    0.0
                };
                let max_amp = recent_samples.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));
                log::info!("📊 [BlackHole] Callback #{}: Buffer size: {} samples ({}s), RMS: {:.6}, Max: {:.6}", 
                    callback_count, buf_size, buf_size as f32 / 48000.0, rms, max_amp);
            }
        };
        
        device.build_input_stream(
            &config.clone().into(),
            stream_cb,
            |err| log::error!("BlackHole stream error: {}", err),
            None,
        )
    }
}

impl SystemAudioCapture for MacOSSystemAudio {
    fn start_capture(&mut self) -> Result<()> {
        // If already capturing, stop first to ensure clean state
        if self.is_capturing {
            log::warn!("⚠️ [SystemAudio] Already capturing, stopping first to ensure clean state...");
            let _ = self.stop_capture();
            // Small delay to ensure cleanup completes
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        // Strategy 1: Try BlackHole first (more reliable)
        if let Some(blackhole_device) = Self::find_blackhole_device() {
            match self.start_blackhole_capture(blackhole_device) {
                Ok(true) => {
                    log::info!("✅ Using BlackHole for system audio capture (audio detected)");
                    return Ok(());
                }
                Ok(false) => {
                    log::warn!("⚠️  BlackHole started but no audio detected initially.");
                    log::info!("💡 [BlackHole] Keeping BlackHole running - will monitor for audio");
                    log::info!("💡 [BlackHole] User can configure Sound Output to 'BlackHole 2ch' and audio will start flowing");
                    
                    // Try to open System Settings to help user configure
                    log::info!("💡 [BlackHole] Attempting to open System Settings > Sound...");
                    let _ = std::process::Command::new("open")
                        .args(["-b", "com.apple.systempreferences", "com.apple.preference.sound"])
                        .output();
                    
                    // Emit log event to frontend
                    let _ = self.app_handle.emit("log-update", format!(
                        "⚠️ [BlackHole] No audio detected. Please set Sound Output to 'BlackHole 2ch' in System Settings > Sound > Output. App will continue monitoring for audio."
                    ));
                    
                    // Keep BlackHole running - don't stop it
                    // Audio may start when user configures Sound Output
                    log::info!("✅ [BlackHole] Keeping capture active - monitoring for audio...");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("⚠️  Failed to start BlackHole capture: {}. Falling back to ScreenCaptureKit.", e);
                }
            }
        }
        
        // Strategy 2: Fallback to ScreenCaptureKit
        log::info!("🔄 Falling back to ScreenCaptureKit...");
        
        // Try to start ScreenCaptureKit helper binary
        // First check in app bundle Resources (for production builds)
        let exe_path = std::env::current_exe()?;
        let mut possible_path = exe_path.clone();
        possible_path.pop(); // MacOS/
        possible_path.pop(); // Contents/
        possible_path.push("Resources/bin/macos-audio-capture");
        
        if !possible_path.exists() {
            // Try from current directory (for dev builds)
            let bin_path = std::env::current_dir()?;
            possible_path = bin_path.clone();
            possible_path.push("bin/macos-audio-capture");

            if !possible_path.exists() {
                // Try src-tauri/bin (if running from root)
                possible_path = bin_path.clone();
                possible_path.push("src-tauri/bin/macos-audio-capture");
            }

            if !possible_path.exists() {
                // Try one level up (if running from target/debug/...)
                let mut bin_path2 = bin_path.clone();
                bin_path2.pop();
                possible_path = bin_path2;
                possible_path.push("src-tauri/bin/macos-audio-capture");
            }
        }

        if possible_path.exists() {
            log::info!("Starting ScreenCaptureKit helper: {:?}", possible_path);

            match Command::new(possible_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()) // Capture stderr for logs
                .spawn()
            {
                Ok(mut child) => {
                    let stdout = child.stdout.take().unwrap();
                    let stderr = child.stderr.take().unwrap();
                    let buffer = self.sample_buffer.clone();
                    let app_handle_audio = self.app_handle.clone();
                    let app_handle_log = self.app_handle.clone();

                    // Thread to read audio data
                    thread::spawn(move || {
                        let mut reader = std::io::BufReader::new(stdout);
                        let mut bytes = [0u8; 4096];
                        let mut chunks_read = 0;

                        loop {
                            match reader.read(&mut bytes) {
                                Ok(n) if n > 0 => {
                                    chunks_read += 1;
                                    // Log first chunk and periodically
                                    if chunks_read == 1 {
                                        log::info!("🎉 [SystemCapture] ✅ First audio data received from SCK helper! ({} bytes)", n);
                                    }
                                    // Log every 100 chunks for debugging (more frequent)
                                    if chunks_read % 100 == 0 {
                                        log::info!("📥 [SystemCapture] Received {} chunks from helper (last chunk: {} bytes)", chunks_read, n);
                                    }

                                           // Convert bytes to f32 (Little Endian)
                                           let float_count = n / 4;
                                           let mut floats = Vec::with_capacity(float_count);
                                           let mut sum_sq = 0.0;

                                           for i in 0..float_count {
                                               let start = i * 4;
                                               let end = start + 4;
                                               // Safety: bytes buffer is large enough
                                               if end <= n {
                                                   let val = f32::from_le_bytes(
                                                       bytes[start..end].try_into().unwrap(),
                                                   );
                                                   floats.push(val);
                                                   sum_sq += val * val;
                                               }
                                           }
                                           
                                           // Log first chunk and periodically
                                           if chunks_read == 1 {
                                               log::info!("📊 [SystemCapture] First chunk: {} floats, buffer size: {}", float_count, buffer.lock().unwrap().len());
                                           }
                                           // Log every 100 chunks for debugging
                                           if chunks_read % 100 == 0 {
                                               let buffer_size = buffer.lock().unwrap().len();
                                               log::info!("📊 [SystemCapture] Added {} floats to buffer (total: {}, chunks: {})", float_count, buffer_size, chunks_read);
                                           }

                                    // Emit levels for visualization
                                    if float_count > 0 {
                                        let rms = (sum_sq / float_count as f32).sqrt();
                                        // Scale up a bit for better visibility
                                        let level = (rms * 5.0).min(1.0);
                                        utils::emit_levels(&app_handle_audio, &vec![level]);
                                    }

                                    let mut guard = buffer.lock().unwrap();
                                    guard.extend(floats);
                                }
                                Ok(_) => break, // EOF
                                Err(_) => break,
                            }
                        }
                        log::info!("SCK helper stdout closed");
                    });

                    // Thread to read logs from stderr
                    let app_handle_open_settings = self.app_handle.clone();
                    let app_handle_log_clone = app_handle_log.clone(); // Clone for this thread
                    thread::spawn(move || {
                        let mut reader = std::io::BufReader::new(stderr);
                        let mut line = String::new();
                        let mut permission_denied_detected = false;
                        while reader.read_line(&mut line).unwrap_or(0) > 0 {
                            let log_line = format!("[SCK Helper] {}", line.trim());
                            log::info!("{}", log_line);
                            
                            // Check for permission denied and auto-open System Settings
                            if (log_line.contains("PERMISSION DENIED") || 
                                log_line.contains("❌ PERMISSION DENIED") ||
                                log_line.contains("declined TCCs")) && !permission_denied_detected {
                                permission_denied_detected = true;
                                log::warn!("⚠️ Permission denied detected! Auto-opening System Settings...");
                                
                                // Mark permission as denied in the struct (need to use Arc<Mutex> for this)
                                // For now, we'll handle this in get_system_audio_status by checking process state
                                
                                // Auto-open System Settings on macOS
                                #[cfg(target_os = "macos")]
                                {
                                    use std::process::Command;
                                    let _ = Command::new("open")
                                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                                        .spawn();
                                    log::info!("✅ Opened System Settings > Privacy & Security > Screen Recording");
                                    let _ = app_handle_open_settings.emit("log-update", "✅ [System] Opened System Settings - Please grant Screen Recording permission for Terminal or Handy".to_string());
                                }
                            }
                            
                            // Emit log to frontend for SystemAudioStatus component
                            // Use clone to ensure we can emit from this thread
                            if let Err(e) = app_handle_log_clone.emit("log-update", log_line.clone()) {
                                log::warn!("Failed to emit log-update event: {}", e);
                            }
                            line.clear();
                        }
                    });

                    // Store process first
                    self.capture_process = Some(child);
                    
                    // Check if process exited immediately (likely permission denied)
                    // Give it a moment to start, then check
                    std::thread::sleep(std::time::Duration::from_millis(2000));
                    
                    // Check process status
                    if let Some(ref mut proc) = self.capture_process {
                        match proc.try_wait() {
                            Ok(Some(status)) => {
                                // Process exited - likely permission denied
                                log::warn!("❌ SCK helper process exited with status: {:?} - Permission likely denied", status);
                                self.permission_denied = true;
                                self.is_capturing = false;
                                self.capture_process = None;
                                return Err(anyhow!("Screen Recording permission denied. Please grant permission in System Settings > Privacy & Security > Screen Recording for Terminal (dev) or Handy (production)."));
                            },
                            Ok(None) => {
                                // Process still running - good!
                                log::info!("✅ SCK helper process is running - permission granted");
                                self.is_capturing = true;
                                self.permission_denied = false;
                                return Ok(());
                            },
                            Err(e) => {
                                log::error!("Failed to check process status: {}", e);
                                // Assume it's running if we can't check
                                self.is_capturing = true;
                                self.permission_denied = false;
                                return Ok(());
                            }
                        }
                    } else {
                        return Err(anyhow!("Failed to store SCK helper process"));
                    }
                }
                Err(e) => {
                    log::error!("Failed to spawn SCK helper: {}", e);
                }
            }
        } else {
            let current_dir = std::env::current_dir().unwrap_or_default();
            log::warn!(
                "SCK helper binary not found. Searched paths relative to: {:?}",
                current_dir
            );
        }

        Err(anyhow!(
            "System Audio Capture failed. Please ensure macOS 13+ and Screen Recording permission is granted."
        ))
    }

    fn stop_capture(&mut self) -> Result<()> {
        if !self.is_capturing {
            log::debug!("[SystemAudio] Not capturing, nothing to stop");
            return Ok(());
        }

        log::info!("🛑 [SystemAudio] Stopping capture (method: {})", 
            if self.use_blackhole { "BlackHole" } else { "ScreenCaptureKit" });

        if self.use_blackhole {
            // Stop BlackHole stream by signaling stop
            if let Some(tx) = self.blackhole_stop_tx.take() {
                log::info!("🛑 [SystemAudio] Signaling BlackHole thread to stop...");
                let _ = tx.send(()); // Signal thread to stop
            }
            // Don't wait for thread to finish - let it finish in background to avoid blocking UI
            // The thread will finish and drop the stream automatically
            if let Some(thread_handle) = self.blackhole_thread.take() {
                log::info!("🛑 [SystemAudio] Signaling BlackHole thread to stop (will finish in background)...");
                // Spawn a background thread to wait for the BlackHole thread to finish
                // This prevents blocking the main thread and causing UI lag
                std::thread::spawn(move || {
                    let _ = thread_handle.join(); // Wait in background thread
                    log::info!("✅ [SystemAudio] BlackHole thread finished (background cleanup)");
                });
            }
        } else {
            // Stop ScreenCaptureKit helper
            if let Some(mut child) = self.capture_process.take() {
                log::info!("🛑 [SystemAudio] Stopping SCK helper process...");
                let _ = child.kill();
                let _ = child.wait();
                log::info!("✅ [SystemAudio] SCK helper process stopped");
            }
        }

        // Clear sample buffer
        {
            let mut buffer = self.sample_buffer.lock().unwrap();
            buffer.clear();
            log::info!("🧹 [SystemAudio] Cleared sample buffer");
        }

        self.is_capturing = false;
        self.use_blackhole = false;
        log::info!("✅ [SystemAudio] Capture stopped successfully");
        Ok(())
    }

    fn read_samples(&mut self) -> Result<Option<Vec<f32>>> {
        let mut buffer = self.sample_buffer.lock().unwrap();

        if buffer.is_empty() {
            // Log periodically to confirm we're checking (every 10 calls)
            static CALL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 10 == 0 {
                log::info!("🔍 [SystemCapture] Buffer is empty (checked {} times) - SCStream may not be sending audio buffers. Please ensure audio is playing from Chrome or another app.", count + 1);
                // Check if process is still running (without mutable borrow)
                let process_running = if let Some(ref mut proc) = self.capture_process {
                    proc.try_wait().ok().flatten().is_none()
                } else {
                    false
                };
                log::info!("🔍 [SystemCapture] Debug: is_capturing={}, process running={}", 
                    self.is_capturing, 
                    process_running);
            }
            return Ok(None);
        }

        // Drain all samples
        let sample_count = buffer.len();
        let samples: Vec<f32> = buffer.drain(..).collect();
        
        // Log periodically (every 100 reads) to avoid spam
        static READ_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = READ_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 100 == 0 {
            let sample_rate = if self.use_blackhole { 48000 } else { 48000 }; // Both are 48kHz typically
            log::info!("✅ [SystemCapture] Read {} samples from buffer ({}s audio at {}kHz) - method: {}", 
                sample_count, 
                sample_count as f32 / sample_rate as f32,
                sample_rate,
                if self.use_blackhole { "BlackHole" } else { "ScreenCaptureKit" }
            );
        }
        Ok(Some(samples))
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}

