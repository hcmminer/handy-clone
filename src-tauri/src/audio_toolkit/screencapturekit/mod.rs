//! ScreenCaptureKit audio capture for macOS 13+
//! 
//! This module provides native system audio capture using Apple's ScreenCaptureKit API,
//! eliminating the need for BlackHole or other virtual audio devices on macOS 13+.

#[cfg(target_os = "macos")]
pub mod capture;

#[cfg(target_os = "macos")]
pub mod permissions;

#[cfg(target_os = "macos")]
pub use capture::ScreenCaptureKitAudio;

#[cfg(target_os = "macos")]
pub use permissions::{check_screen_recording_permission, request_screen_recording_permission};

#[cfg(not(target_os = "macos"))]
pub struct ScreenCaptureKitAudio;

#[cfg(not(target_os = "macos"))]
impl ScreenCaptureKitAudio {
    pub fn new(_sender: crossbeam_channel::Sender<Vec<f32>>) -> Result<Self, String> {
        Err("ScreenCaptureKit is only available on macOS".to_string())
    }
    
    pub fn start_capture(&mut self) -> Result<(), String> {
        Err("ScreenCaptureKit is only available on macOS".to_string())
    }
    
    pub fn stop_capture(&mut self) -> Result<(), String> {
        Err("ScreenCaptureKit is only available on macOS".to_string())
    }
}
