// macOS System Audio Capture using ScreenCaptureKit
// This requires macOS 13+ (Ventura)

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::io::{BufRead, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::audio_toolkit::system_audio::SystemAudioCapture;
use crate::utils;
use tauri::{AppHandle, Emitter};

/// macOS implementation using ScreenCaptureKit
pub struct MacOSSystemAudio {
    is_capturing: bool,
    permission_denied: bool, // Track if permission was denied
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    capture_process: Option<Child>,
    app_handle: AppHandle,
}

impl MacOSSystemAudio {
    pub fn new(app: &AppHandle) -> Result<Self> {
        Ok(Self {
            is_capturing: false,
            permission_denied: false,
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            capture_process: None,
            app_handle: app.clone(),
        })
    }
    
    pub fn is_permission_denied(&self) -> bool {
        self.permission_denied
    }
}

impl SystemAudioCapture for MacOSSystemAudio {
    fn start_capture(&mut self) -> Result<()> {
        if self.is_capturing {
            return Ok(());
        }

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

        if let Some(mut child) = self.capture_process.take() {
            log::info!("Stopping SCK helper");
            let _ = child.kill();
            let _ = child.wait();
        }

        self.is_capturing = false;
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
        // Always log when we read samples - this is important
        log::info!("✅ [SystemCapture] Read {} samples from buffer ({}s audio at 48kHz)", sample_count, sample_count as f32 / 48000.0);
        Ok(Some(samples))
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}

