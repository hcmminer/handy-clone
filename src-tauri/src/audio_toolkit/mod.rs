pub mod audio;
pub mod constants;
pub mod system_audio;
pub mod text;
pub mod utils;
pub mod vad;

#[cfg(target_os = "macos")]
pub mod screencapturekit;

pub use audio::{
    list_input_devices, list_output_devices, save_wav_file, AudioRecorder, CpalDeviceInfo,
};

#[cfg(target_os = "macos")]
pub use system_audio::{SystemAudioCapture, MacOSSystemAudio};

#[cfg(target_os = "macos")]
pub use screencapturekit::ScreenCaptureKitAudio;

#[cfg(target_os = "windows")]
pub use system_audio::{SystemAudioCapture, WindowsSystemAudio};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use system_audio::SystemAudioCapture;
pub use text::apply_custom_words;
pub use utils::get_cpal_host;
pub use vad::{SileroVad, VoiceActivityDetector};
