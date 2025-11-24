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
    fn find_blackhole_device() -> Option<Device> {
        let host = crate::audio_toolkit::get_cpal_host();
        log::info!("🔍 [BlackHole] Enumerating input devices...");
        
        // Also check default input device
        if let Some(default_input) = host.default_input_device() {
            if let Ok(name) = default_input.name() {
                log::info!("🔍 [BlackHole] Default input device: {}", name);
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
            
            // Check default output
            if let Some(default_output) = host.default_output_device() {
                if let Ok(name) = default_output.name() {
                    log::info!("🔍 [BlackHole] Default output device: {}", name);
                    if !name.contains("BlackHole") && !name.contains("blackhole") {
                        log::warn!("⚠️ [BlackHole] Default output is NOT BlackHole (current: {}). Audio will not be routed to BlackHole!", name);
                        log::warn!("⚠️ [BlackHole] Please set Sound Output to 'BlackHole 2ch' in System Settings > Sound > Output");
                    } else {
                        log::info!("✅ [BlackHole] Default output is BlackHole - audio should be routed correctly");
                    }
                }
            }
        }
        
        if let Ok(devices) = host.input_devices() {
            let mut device_list = Vec::new();
            for device in devices {
                if let Ok(name) = device.name() {
                    device_list.push(name.clone());
                    log::info!("🔍 [BlackHole] Found input device: {}", name);
                    if name.contains("BlackHole") || name.contains("blackhole") {
                        log::info!("✅ [BlackHole] Found BlackHole device: {}", name);
                        return Some(device);
                    }
                }
            }
            log::info!("📋 [BlackHole] All input devices: {:?}", device_list);
        } else {
            log::warn!("⚠️ [BlackHole] Failed to enumerate input devices");
        }
        log::info!("⚠️ [BlackHole] BlackHole device not found. Will try ScreenCaptureKit.");
        None
    }
    
    /// Start capture from BlackHole device
    fn start_blackhole_capture(&mut self, device: Device) -> Result<()> {
        log::info!("🎯 Starting BlackHole capture...");
        
        let config = device.default_input_config()
            .map_err(|e| anyhow!("Failed to get BlackHole config: {}", e))?;
        
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        
        log::info!("📊 BlackHole config: sample_rate={}, channels={}, format={:?}", 
            sample_rate, channels, config.sample_format());
        
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
        
        log::info!("✅ BlackHole capture started successfully!");
        Ok(())
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
            if callback_count <= 5 {
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
                log::info!("🎵 [BlackHole] Callback #{}: {} samples, RMS: {:.6}, Max: {:.6}", 
                    callback_count, data.len(), rms, max_amp);
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
                log::info!("📊 [BlackHole] Callback #{}: Buffer size: {} samples ({}s)", 
                    callback_count, buf_size, buf_size as f32 / 48000.0);
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
        if self.is_capturing {
            return Ok(());
        }

        // Strategy 1: Try BlackHole first (more reliable)
        if let Some(blackhole_device) = Self::find_blackhole_device() {
            match self.start_blackhole_capture(blackhole_device) {
                Ok(()) => {
                    log::info!("✅ Using BlackHole for system audio capture");
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
            return Ok(());
        }

        if self.use_blackhole {
            // Stop BlackHole stream by signaling stop
            if let Some(tx) = self.blackhole_stop_tx.take() {
                log::info!("Stopping BlackHole stream");
                let _ = tx.send(()); // Signal thread to stop
            }
            // Wait for thread to finish
            if let Some(thread_handle) = self.blackhole_thread.take() {
                let _ = thread_handle.join(); // Wait for thread to finish (drops stream)
            }
        } else {
            // Stop ScreenCaptureKit helper
            if let Some(mut child) = self.capture_process.take() {
                log::info!("Stopping SCK helper");
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        self.is_capturing = false;
        self.use_blackhole = false;
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

