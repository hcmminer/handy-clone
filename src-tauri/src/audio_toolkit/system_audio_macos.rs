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
use tauri::AppHandle;

/// macOS implementation using ScreenCaptureKit
pub struct MacOSSystemAudio {
    is_capturing: bool,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    capture_process: Option<Child>,
    app_handle: AppHandle,
}

impl MacOSSystemAudio {
    pub fn new(app: &AppHandle) -> Result<Self> {
        Ok(Self {
            is_capturing: false,
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            capture_process: None,
            app_handle: app.clone(),
        })
    }
}

impl SystemAudioCapture for MacOSSystemAudio {
    fn start_capture(&mut self) -> Result<()> {
        if self.is_capturing {
            return Ok(());
        }

        // Try to start ScreenCaptureKit helper binary
        let bin_path = std::env::current_dir()?;

        // Check if we are in src-tauri or root
        let mut possible_path = bin_path.clone();
        possible_path.push("bin/macos-audio-capture");

        if !possible_path.exists() {
            // Try src-tauri/bin (if running from root)
            possible_path = bin_path.clone();
            possible_path.push("src-tauri/bin/macos-audio-capture");
        }

        // Final check
        if !possible_path.exists() {
            // Try one level up (if running from target/debug/...)
            possible_path = bin_path.clone();
            possible_path.pop();
            possible_path.push("src-tauri/bin/macos-audio-capture");
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
                    let app_handle = self.app_handle.clone();

                    // Thread to read audio data
                    thread::spawn(move || {
                        let mut reader = std::io::BufReader::new(stdout);
                        let mut bytes = [0u8; 4096];
                        let mut chunks_read = 0;

                        loop {
                            match reader.read(&mut bytes) {
                                Ok(n) if n > 0 => {
                                    chunks_read += 1;
                                    // Reduce log frequency - only log every 500 chunks
                                    if chunks_read % 500 == 0 {
                                        log::info!("Rust received {} chunks from helper", chunks_read);
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
                                           
                                           // Log periodically - only every 1000 chunks to reduce spam
                                           if chunks_read % 1000 == 0 {
                                               log::debug!("Rust: Added {} floats to buffer (total buffer size: {})", float_count, buffer.lock().unwrap().len());
                                           }

                                    // Emit levels for visualization
                                    if float_count > 0 {
                                        let rms = (sum_sq / float_count as f32).sqrt();
                                        // Scale up a bit for better visibility
                                        let level = (rms * 5.0).min(1.0);
                                        utils::emit_levels(&app_handle, &vec![level]);
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
                    thread::spawn(move || {
                        let mut reader = std::io::BufReader::new(stderr);
                        let mut line = String::new();
                        while reader.read_line(&mut line).unwrap_or(0) > 0 {
                            log::info!("[SCK Helper] {}", line.trim());
                            line.clear();
                        }
                    });

                    self.capture_process = Some(child);
                    self.is_capturing = true;
                    return Ok(());
                }
                Err(e) => {
                    log::error!("Failed to spawn SCK helper: {}", e);
                }
            }
        } else {
            log::warn!(
                "SCK helper binary not found. Searched paths relative to: {:?}",
                bin_path
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
                log::debug!("System capture buffer is empty (checked {} times)", count + 1);
            }
            return Ok(None);
        }

        // Drain all samples
        let samples: Vec<f32> = buffer.drain(..).collect();
        // Always log when we read samples - this is important
        log::info!("✅ System capture read {} samples from buffer", samples.len());
        Ok(Some(samples))
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}

