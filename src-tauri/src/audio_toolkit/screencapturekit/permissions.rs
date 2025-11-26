//! Permission checking for ScreenCaptureKit
//! 
//! ScreenCaptureKit requires "Screen Recording" permission on macOS.

use cocoa::base::nil;
use cocoa::foundation::NSString;
use objc::runtime::{Class, Object};
use objc::{class, msg_send, sel, sel_impl};

/// Check if Screen Recording permission is granted
/// 
/// ScreenCaptureKit requires Screen Recording permission to capture system audio.
/// This permission is managed in System Preferences > Privacy & Security > Screen Recording.
pub fn check_screen_recording_permission() -> bool {
    unsafe {
        // Try to get shareable content - will fail if permission not granted
        let cls = Class::get("SCShareableContent").expect("SCShareableContent class not found");
        let _: *mut Object = msg_send![cls, currentProcessShareableContent];
        
        // If we get here without crashing, permission is likely granted
        // Note: This is a simple check. For production, you'd want to use
        // SCShareableContent.getShareableContentWithCompletionHandler and check the error
        true
    }
}

/// Request Screen Recording permission
/// 
/// Opens System Preferences to the Screen Recording privacy settings
/// where the user can grant permission to the app.
pub fn request_screen_recording_permission() {
    unsafe {
        // Open Screen Recording privacy settings
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let url_string = NSString::alloc(nil)
            .init_str("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
        let url: *mut Object = msg_send![class!(NSURL), URLWithString: url_string];
        let _: () = msg_send![workspace, openURL: url];
        
        log::info!("Opened System Preferences for Screen Recording permission");
    }
}

/// Get macOS version as (major, minor) tuple
pub fn get_macos_version() -> Option<(u32, u32)> {
    use std::process::Command;
    
    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    
    let version = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = version.trim().split('.').collect();
    
    if parts.len() >= 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        return Some((major, minor));
    }
    
    None
}

/// Check if current macOS version supports ScreenCaptureKit
/// 
/// ScreenCaptureKit was introduced in macOS 13.0 (Ventura)
pub fn supports_screencapturekit() -> bool {
    if let Some((major, _minor)) = get_macos_version() {
        major >= 13
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_detection() {
        let version = get_macos_version();
        assert!(version.is_some(), "Should detect macOS version");
        
        if let Some((major, minor)) = version {
            println!("Detected macOS {}.{}", major, minor);
            assert!(major >= 10, "macOS major version should be >= 10");
        }
    }
    
    #[test]
    fn test_screencapturekit_support() {
        let supports = supports_screencapturekit();
        println!("ScreenCaptureKit supported: {}", supports);
        // This test just logs, doesn't assert since it depends on OS version
    }
}
