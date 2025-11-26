// Permission-related commands

#[tauri::command]
pub fn get_macos_version() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::permissions::get_macos_version;
        
        if let Some((major, minor)) = get_macos_version() {
            Ok(format!("{}.{}", major, minor))
        } else {
            Err("Failed to get macOS version".to_string())
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Err("Not running on macOS".to_string())
    }
}

#[tauri::command]
pub fn supports_screencapturekit() -> bool {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::permissions::supports_screencapturekit;
        supports_screencapturekit()
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::permissions::check_screen_recording_permission;
        check_screen_recording_permission()
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
pub fn request_screen_recording_permission() {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::permissions::request_screen_recording_permission;
        request_screen_recording_permission();
    }
}
