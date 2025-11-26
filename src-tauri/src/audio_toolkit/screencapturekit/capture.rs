//! ScreenCaptureKit audio capture implementation
//! 
//! Captures system audio using Apple's ScreenCaptureKit API.
//! Audio-only mode (no video frames) to minimize performance impact.

use screencapturekit::{
    shareable_content::SCShareableContent,
    stream::{
        configuration::SCStreamConfiguration,
        content_filter::SCContentFilter,
        output_trait::SCStreamOutputTrait,
        output_type::SCStreamOutputType,
        SCStream,
    },
};
use core_media_rs::cm_sample_buffer::CMSampleBuffer;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use anyhow::Result;

use crate::audio_toolkit::SystemAudioCapture;

/// Audio output handler for ScreenCaptureKit
struct AudioStreamOutput {
    buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl SCStreamOutputTrait for AudioStreamOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        // Only process audio samples
        if of_type != SCStreamOutputType::Audio {
            log::trace!("📹 [SCK] Received non-audio buffer type: {:?}", of_type);
            return;
        }
        
        
        // Extract audio data from CMSampleBuffer
        match sample_buffer.get_audio_buffer_list() {
            Ok(audio_buffer_list) => {
                let num_buffers = audio_buffer_list.num_buffers();
                
                let mut buffer_lock = self.buffer.lock().unwrap();
                let before_len = buffer_lock.len();
                
                for i in 0..num_buffers {
                    if let Some(buffer) = audio_buffer_list.get(i) {
                        let data = buffer.data();
                        let data_len = data.len();
                        
                        // Convert bytes to f32 samples
                        // ScreenCaptureKit outputs Float32 PCM
                        let mut samples_added = 0;
                        for chunk in data.chunks_exact(4) {
                            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            buffer_lock.push_back(sample);
                            samples_added += 1;
                        }
                        
                    } else {
                        log::warn!("⚠️ [SCK] Buffer[{}] is None", i);
                    }
                }
                
                let after_len = buffer_lock.len();
                let added = after_len - before_len;
            }
            Err(e) => {
                log::error!("❌ [SCK] Failed to get audio buffer list: {:?}", e);
            }
        }
            match sample_buffer.get_audio_buffer_list() {
                Ok(audio_buffer_list) => {
                    let num_buffers = audio_buffer_list.num_buffers();
                    let mut buffer_lock = self.buffer.lock().unwrap();
                    for i in 0..num_buffers {
                        if let Some(buffer) = audio_buffer_list.get(i) {
                            let data = buffer.data();
                            for chunk in data.chunks_exact(4) {
                                let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                                buffer_lock.push_back(sample);
                            }
                        } else {
                            log::warn!("⚠️ [SCK] Buffer[{}] is None", i);
                        }
                    }
                    // Only log if buffer is empty or error
                    if buffer_lock.is_empty() {
                        log::warn!("⚠️ [SCK] Audio buffer is empty after callback");
                    }
                }
                Err(e) => {
                    log::error!("❌ [SCK] Failed to get audio buffer list: {:?}", e);
                }
            }
    }
}

/// ScreenCaptureKit audio capture
/// 
/// Captures system audio using ScreenCaptureKit API.
/// Audio-only mode: captures at 48kHz stereo, minimal CPU/GPU usage.
pub struct ScreenCaptureKitAudio {
    stream: Arc<Mutex<Option<SCStream>>>,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    is_capturing: Arc<Mutex<bool>>,
}

impl ScreenCaptureKitAudio {
    /// Create a new ScreenCaptureKit audio capture instance
    pub fn new(_app_handle: &tauri::AppHandle) -> Result<Self> {
        log::info!("Initializing ScreenCaptureKit audio capture");
        
        Ok(Self {
            stream: Arc::new(Mutex::new(None)),
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            is_capturing: Arc::new(Mutex::new(false)),
        })
    }
}

impl SystemAudioCapture for ScreenCaptureKitAudio {
    /// Start capturing system audio
    /// 
    /// Captures audio-only from the primary display.
    /// Configuration:
    /// - Sample rate: 48kHz
    /// - Channels: 2 (stereo)
    /// - Format: Float32 PCM
    /// - Video: Minimal resolution (2x2) to avoid rendering overhead
    fn start_capture(&mut self) -> Result<()> {
        log::info!("🚀 [SCK] Starting ScreenCaptureKit audio capture");
        
        // Check if already capturing
        {
            let is_capturing = self.is_capturing.lock().unwrap();
            if *is_capturing {
                log::warn!("⚠️ [SCK] Already capturing, skipping start");
                return Ok(());
            }
        }
        
        // Configure stream for audio-only capture
        let config = SCStreamConfiguration::new()
            .set_captures_audio(true)
            .map_err(|e| anyhow::anyhow!("❌ Failed to enable audio capture: {:?}", e))?
            .set_sample_rate(48000)
            .map_err(|e| anyhow::anyhow!("❌ Failed to set sample rate: {:?}", e))?
            .set_channel_count(2)
            .map_err(|e| anyhow::anyhow!("❌ Failed to set channel count: {:?}", e))?;
        
        log::info!("✅ [SCK] Stream configured: 48kHz, 2 channels, audio-only");
        
        // Get the primary display
        let shareable_content = SCShareableContent::get()
            .map_err(|e| anyhow::anyhow!("❌ Failed to get shareable content: {:?}. Make sure Screen Recording permission is granted.", e))?;
        
        let mut displays = shareable_content.displays();
        if displays.is_empty() {
            log::error!("❌ [SCK] No displays available for capture");
            return Err(anyhow::anyhow!("No displays available for capture"));
        }
        
        let display = displays.remove(0);
        log::info!("✅ [SCK] Capturing audio from display ID: {}", display.display_id());
        
        // Create content filter - capture all system audio from the display
        // We use display capture (not window) to get all system audio
        let filter = SCContentFilter::new()
            .with_display_excluding_windows(&display, &[]);
        
        log::info!("✅ [SCK] Content filter created - capturing all system audio");
        
        // Create stream with audio output handler
        let mut stream = SCStream::new(&filter, &config);
        
        // Add output handler for audio
        let output_handler = AudioStreamOutput {
            buffer: self.audio_buffer.clone(),
        };
        
        stream.add_output_handler(output_handler, SCStreamOutputType::Audio);
        log::info!("✅ [SCK] Audio output handler added");
        
        // Start capturing
        log::info!("▶️ [SCK] Starting capture...");
        stream.start_capture()
            .map_err(|e| anyhow::anyhow!("❌ Failed to start capture: {:?}", e))?;
        
        // Store stream and set capturing flag
        *self.stream.lock().unwrap() = Some(stream);
        *self.is_capturing.lock().unwrap() = true;
        
        log::info!("🎉 [SCK] ScreenCaptureKit audio capture started successfully!");
        log::info!("👂 [SCK] Listening for system audio... Callbacks should start appearing now.");
        Ok(())
    }
    
    /// Stop capturing audio
    fn stop_capture(&mut self) -> Result<()> {
        log::info!("Stopping ScreenCaptureKit audio capture");
        
        let mut stream_lock = self.stream.lock().unwrap();
        
        if let Some(stream) = stream_lock.take() {
            stream.stop_capture()
                .map_err(|e| anyhow::anyhow!("Failed to stop capture: {:?}", e))?;
            *self.is_capturing.lock().unwrap() = false;
            log::info!("ScreenCaptureKit audio capture stopped");
        }
        
        Ok(())
    }
    
    /// Read available audio samples (non-blocking)
    /// 
    /// Returns samples captured at 48kHz stereo.
    /// Caller should resample to 16kHz mono for Whisper.
    fn read_samples(&mut self) -> Result<Option<Vec<f32>>> {
        let mut buffer = self.audio_buffer.lock().unwrap();
        
        if buffer.is_empty() {
                // Only log if buffer is empty
                log::warn!("📭 [SCK] read_samples: Buffer is empty");
            return Ok(None);
        }
        
        // Drain all available samples
        let samples: Vec<f32> = buffer.drain(..).collect();
        let sample_count = samples.len();
        let duration_secs = sample_count as f32 / 48000.0;
        
        log::info!("📤 [SCK] read_samples: Returning {} samples (~{:.2}s audio @ 48kHz)", 
            sample_count, duration_secs);
        
        Ok(Some(samples))
    }
    
    /// Check if currently capturing
    fn is_capturing(&self) -> bool {
        *self.is_capturing.lock().unwrap()
    }
}

impl Drop for ScreenCaptureKitAudio {
    fn drop(&mut self) {
        // Ensure stream is stopped when dropped
        let _ = self.stop_capture();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_audio_capture() {
        // This test requires a Tauri AppHandle which isn't available in unit tests
        // Just verify the module compiles
    }
}

