// System Audio Capture trait
// Platform-specific implementations provide system audio capture functionality

use anyhow::Result;

/// Trait for system audio capture implementations
pub trait SystemAudioCapture: Send + Sync {
    /// Start capturing system audio
    fn start_capture(&mut self) -> Result<()>;
    
    /// Stop capturing system audio
    fn stop_capture(&mut self) -> Result<()>;
    
    /// Read available audio samples (non-blocking)
    /// Returns None if no samples available, Some(Vec<f32>) with samples otherwise
    fn read_samples(&mut self) -> Result<Option<Vec<f32>>>;
    
    /// Check if currently capturing
    fn is_capturing(&self) -> bool;
}

#[cfg(target_os = "macos")]
pub use system_audio_macos::MacOSSystemAudio;

#[cfg(target_os = "macos")]
#[path = "system_audio_macos.rs"]
mod system_audio_macos;

#[cfg(not(target_os = "macos"))]
pub struct DummySystemAudio;

#[cfg(not(target_os = "macos"))]
impl SystemAudioCapture for DummySystemAudio {
    fn start_capture(&mut self) -> Result<()> {
        Err(anyhow::anyhow!("System audio capture not supported on this platform"))
    }
    
    fn stop_capture(&mut self) -> Result<()> {
        Ok(())
    }
    
    fn read_samples(&mut self) -> Result<Option<Vec<f32>>> {
        Ok(None)
    }
    
    fn is_capturing(&self) -> bool {
        false
    }
}

