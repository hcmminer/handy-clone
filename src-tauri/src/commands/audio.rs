use crate::audio_feedback;
use crate::audio_toolkit::audio::{list_input_devices, list_output_devices};
use crate::managers::audio::{AudioRecordingManager, MicrophoneMode};
use crate::settings::{get_settings, write_settings, AudioSource};
use log::warn;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize)]
pub struct CustomSounds {
    start: bool,
    stop: bool,
}

fn custom_sound_exists(app: &AppHandle, sound_type: &str) -> bool {
    app.path()
        .resolve(
            format!("custom_{}.wav", sound_type),
            tauri::path::BaseDirectory::AppData,
        )
        .map_or(false, |path| path.exists())
}

#[tauri::command]
pub fn check_custom_sounds(app: AppHandle) -> CustomSounds {
    CustomSounds {
        start: custom_sound_exists(&app, "start"),
        stop: custom_sound_exists(&app, "stop"),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioDevice {
    pub index: String,
    pub name: String,
    pub is_default: bool,
}

#[tauri::command]
pub fn update_microphone_mode(app: AppHandle, always_on: bool) -> Result<(), String> {
    // Update settings
    let mut settings = get_settings(&app);
    settings.always_on_microphone = always_on;
    write_settings(&app, settings);

    // Update the audio manager mode
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let new_mode = if always_on {
        MicrophoneMode::AlwaysOn
    } else {
        MicrophoneMode::OnDemand
    };

    rm.update_mode(new_mode)
        .map_err(|e| format!("Failed to update microphone mode: {}", e))
}

#[tauri::command]
pub fn get_microphone_mode(app: AppHandle) -> Result<bool, String> {
    let settings = get_settings(&app);
    Ok(settings.always_on_microphone)
}

#[tauri::command]
pub fn get_available_microphones() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_input_devices().map_err(|e| format!("Failed to list audio devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
pub fn set_selected_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);

    // Update the audio manager to use the new device
    let rm = app.state::<Arc<AudioRecordingManager>>();
    rm.update_selected_device()
        .map_err(|e| format!("Failed to update selected device: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn get_selected_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
pub fn get_available_output_devices() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_output_devices().map_err(|e| format!("Failed to list output devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
pub fn set_selected_output_device(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_output_device = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
pub fn get_selected_output_device(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_output_device
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
pub fn play_test_sound(app: AppHandle, sound_type: String) {
    let sound = match sound_type.as_str() {
        "start" => audio_feedback::SoundType::Start,
        "stop" => audio_feedback::SoundType::Stop,
        _ => {
            warn!("Unknown sound type: {}", sound_type);
            return;
        }
    };
    audio_feedback::play_test_sound(&app, sound);
}

#[tauri::command]
pub fn set_clamshell_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.clamshell_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
pub fn get_clamshell_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .clamshell_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
pub async fn set_audio_source(app: AppHandle, source: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    let audio_source = match source.as_str() {
        "microphone" => Some(AudioSource::Microphone),
        "system_audio" => Some(AudioSource::SystemAudio),
        _ => None,
    };
    settings.audio_source = audio_source;
    write_settings(&app, settings);

    // Update the audio manager to use the new source
    // Spawn in background thread to avoid blocking UI
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let rm_clone = Arc::clone(&rm);
    let app_clone = app.clone();
    
    tauri::async_runtime::spawn(async move {
        if let Err(e) = rm_clone.update_selected_device() {
            log::error!("Failed to update audio source: {}", e);
            // Emit error event to frontend
            let _ = app_clone.emit("log-update", format!("❌ [AudioSource] Failed to update: {}", e));
        }
    });

    // Return immediately to avoid blocking UI
    Ok(())
}

#[tauri::command]
pub fn get_audio_source(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(match settings.audio_source {
        Some(AudioSource::SystemAudio) => "system_audio".to_string(),
        _ => "microphone".to_string(),
    })
}

#[derive(Serialize)]
pub struct SystemAudioStatus {
    pub permission: String, // "unknown" | "granted" | "denied"
    pub capture: String,    // "unknown" | "active" | "waiting" | "error"
    pub audio_detection: String, // "unknown" | "active" | "waiting"
}

#[tauri::command]
pub fn get_system_audio_status(app: AppHandle) -> Result<SystemAudioStatus, String> {
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let (is_open, has_audio) = rm.get_system_audio_status();
    
    // Check if permission was denied by checking if capture failed to start
    // If is_open is false, it could mean permission denied or just not started
    // We'll rely on log events to determine actual permission status
    // For now, if is_open is false, we can't determine permission status
    let permission_denied = false; // Will be determined by log events in frontend
    
    // Determine status
    let capture_status = if is_open {
        "active"
    } else if permission_denied {
        "error" // Permission denied
    } else {
        "unknown"
    };
    
    let audio_detection_status = if is_open {
        if has_audio {
            "active"
        } else {
            "waiting"
        }
    } else {
        "unknown"
    };
    
    // Permission status - check if process exited (permission denied) or if capture is active (granted)
    let permission_status = if permission_denied {
        "denied" // Process exited, permission denied
    } else if is_open {
        "granted" // If capture is active, permission must be granted
    } else {
        "unknown" // If capture is not active, we can't determine permission status
    };
    
    Ok(SystemAudioStatus {
        permission: permission_status.to_string(),
        capture: capture_status.to_string(),
        audio_detection: audio_detection_status.to_string(),
    })
}
