# ScreenCaptureKit Migration Plan for Handy

## Objective

Migrate Handy's system audio capture from BlackHole (requires manual installation) to ScreenCaptureKit (native macOS 13+ API) for better user experience.

## Current Architecture

```
User Input (Keyboard Shortcut)
    ↓
Audio Manager
    ↓
BlackHole Virtual Audio Device
    ↓
CPAL Audio Recording
    ↓
VAD (Voice Activity Detection)
    ↓
Whisper Transcription
    ↓
Clipboard Output
```

## Target Architecture (macOS 13+)

```
User Input (Keyboard Shortcut)
    ↓
Audio Manager
    ↓
[Version Check: macOS >= 13?]
    ↓
YES: ScreenCaptureKit API ──→ Audio Samples (48kHz)
NO:  BlackHole Fallback   ──→ Audio Samples (48kHz)
    ↓
Resampler (48kHz → 16kHz)
    ↓
VAD (Voice Activity Detection)
    ↓
Whisper Transcription
    ↓
Clipboard Output
```

## Implementation Phases

### Phase 1: Research & Setup ✅ (COMPLETED)

- [x] Research ScreenCaptureKit implementations
- [x] Study Azayaka Swift code
- [x] Study screencapturekit-rs Rust bindings
- [x] Document findings

### Phase 2: Add Rust Bindings

**Files to Create:**

```
src-tauri/
├── Cargo.toml (update dependencies)
└── src/
    └── audio_toolkit/
        └── screencapturekit/
            ├── mod.rs          # Main module
            ├── capture.rs      # Audio capture logic
            └── permissions.rs  # Permission handling
```

**Dependencies to Add:**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
screencapturekit = { version = "0.3", optional = true }
core-media-rs = { version = "0.1", optional = true }

[features]
screencapturekit = ["dep:screencapturekit", "dep:core-media-rs"]
```

**Tasks:**

1. Add screencapturekit-rs dependency
2. Create basic capture module
3. Test audio capture independently
4. Verify sample format matches expectations

### Phase 3: Implement ScreenCaptureKit Audio Module

**File: `src-tauri/src/audio_toolkit/screencapturekit/mod.rs`**

```rust
pub mod capture;
pub mod permissions;

pub use capture::ScreenCaptureKitAudio;
pub use permissions::check_screen_recording_permission;
```

**File: `src-tauri/src/audio_toolkit/screencapturekit/capture.rs`**

Key components:

1. **ScreenCaptureKitAudio struct**
   - Manages SCStream lifecycle
   - Handles audio sample buffers
   - Converts to f32 PCM format
   - Resamples 48kHz → 16kHz

2. **Output Handler**
   - Implements SCStreamOutputTrait
   - Processes CMSampleBuffer
   - Sends to audio pipeline

3. **Error Handling**
   - Stream creation failures
   - Permission errors
   - Buffer conversion errors

**File: `src-tauri/src/audio_toolkit/screencapturekit/permissions.rs`**

```rust
#[cfg(target_os = "macos")]
pub fn check_screen_recording_permission() -> bool {
    // Check if Screen Recording permission is granted
    // Required for ScreenCaptureKit
}

#[cfg(target_os = "macos")]
pub fn request_screen_recording_permission() {
    // Open System Preferences to Screen Recording
}
```

### Phase 4: Integrate with Audio Manager

**File: `src-tauri/src/managers/audio.rs`**

**Changes:**

1. **Add version detection:**

```rust
#[cfg(target_os = "macos")]
fn get_macos_version() -> Option<(u32, u32)> {
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

#[cfg(target_os = "macos")]
fn should_use_screencapturekit() -> bool {
    if let Some((major, _)) = get_macos_version() {
        major >= 13  // macOS Ventura+
    } else {
        false
    }
}
```

2. **Add audio source enum:**

```rust
pub enum SystemAudioSource {
    #[cfg(target_os = "macos")]
    ScreenCaptureKit(ScreenCaptureKitAudio),
    
    #[cfg(target_os = "macos")]
    BlackHole(BlackHoleAudio),
    
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}
```

3. **Update AudioManager initialization:**

```rust
impl AudioManager {
    pub fn new() -> Result<Self, AudioError> {
        let audio_source = if cfg!(target_os = "macos") {
            if should_use_screencapturekit() {
                log::info!("Using ScreenCaptureKit for system audio");
                SystemAudioSource::ScreenCaptureKit(
                    ScreenCaptureKitAudio::new()?
                )
            } else {
                log::info!("Using BlackHole for system audio (macOS < 13)");
                SystemAudioSource::BlackHole(
                    BlackHoleAudio::new()?
                )
            }
        } else {
            SystemAudioSource::Unsupported
        };
        
        Ok(Self {
            audio_source,
            // ... other fields
        })
    }
}
```

### Phase 5: Update Permission Handling

**File: `src-tauri/src/commands/permissions.rs`**

Add new command:

```rust
#[tauri::command]
pub async fn check_screencapturekit_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::check_screen_recording_permission;
        Ok(check_screen_recording_permission())
    }
    
    #[cfg(not(target_os = "macos"))]
    Ok(false)
}

#[tauri::command]
pub async fn request_screencapturekit_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio_toolkit::screencapturekit::request_screen_recording_permission;
        request_screen_recording_permission();
        Ok(())
    }
    
    #[cfg(not(target_os = "macos"))]
    Ok(())
}
```

### Phase 6: Update Frontend UI

**File: `src/components/settings/SystemAudioSettings.tsx`**

New component to show audio source:

```tsx
export function SystemAudioSettings() {
  const [audioSource, setAudioSource] = useState<'screencapturekit' | 'blackhole' | 'unknown'>('unknown');
  const [macOSVersion, setMacOSVersion] = useState<string>('');

  useEffect(() => {
    async function checkAudioSource() {
      const version = await invoke<string>('get_macos_version');
      setMacOSVersion(version);
      
      const [major] = version.split('.').map(Number);
      if (major >= 13) {
        const hasPermission = await invoke<boolean>('check_screencapturekit_permission');
        setAudioSource(hasPermission ? 'screencapturekit' : 'unknown');
      } else {
        setAudioSource('blackhole');
      }
    }
    
    checkAudioSource();
  }, []);

  return (
    <div className="setting-item">
      <h3>System Audio Capture</h3>
      
      {audioSource === 'screencapturekit' && (
        <div className="status-box success">
          <CheckIcon />
          <div>
            <p><strong>Using ScreenCaptureKit</strong></p>
            <p>Native system audio capture (macOS {macOSVersion})</p>
            <p className="text-sm text-gray-500">No additional software required</p>
          </div>
        </div>
      )}
      
      {audioSource === 'blackhole' && (
        <div className="status-box warning">
          <InfoIcon />
          <div>
            <p><strong>Using BlackHole</strong></p>
            <p>Your macOS version ({macOSVersion}) requires BlackHole for system audio</p>
            <a href="https://github.com/ExistentialAudio/BlackHole" target="_blank">
              Install BlackHole
            </a>
          </div>
        </div>
      )}
      
      {audioSource === 'unknown' && (
        <div className="status-box error">
          <AlertIcon />
          <div>
            <p><strong>Permission Required</strong></p>
            <p>Handy needs Screen Recording permission for system audio capture</p>
            <button onClick={requestPermission}>
              Grant Permission
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
```

**File: `src/components/onboarding/Onboarding.tsx`**

Update permission step to handle ScreenCaptureKit:

```tsx
function PermissionStep() {
  const [macOSVersion, setMacOSVersion] = useState<number>(0);
  
  // Show different permission UI based on macOS version
  if (macOSVersion >= 13) {
    return <ScreenCaptureKitPermissionUI />;
  } else {
    return <BlackHoleInstallationUI />;
  }
}
```

### Phase 7: Update Entitlements & Info.plist

**File: `src-tauri/Entitlements.plist`**

Add Screen Recording permission:

```xml
<key>com.apple.security.device.audio-input</key>
<true/>

<!-- Add this for ScreenCaptureKit -->
<key>com.apple.security.device.camera</key>
<false/>
```

**File: `src-tauri/Info.plist`**

```xml
<key>NSScreenCaptureUsageDescription</key>
<string>Handy needs screen recording permission to capture system audio for transcription</string>

<key>NSMicrophoneUsageDescription</key>
<string>Handy needs microphone access to transcribe your speech</string>
```

### Phase 8: Testing Plan

**Test Cases:**

1. **macOS 13+ with ScreenCaptureKit:**
   - [ ] Audio capture works without BlackHole
   - [ ] Permission prompt appears correctly
   - [ ] Audio quality matches BlackHole
   - [ ] VAD works correctly with new audio source
   - [ ] Transcription accuracy is maintained

2. **macOS 12 and below:**
   - [ ] Falls back to BlackHole automatically
   - [ ] No ScreenCaptureKit errors
   - [ ] Existing functionality preserved

3. **Permission Handling:**
   - [ ] Permission request shows System Preferences
   - [ ] Permission denial handled gracefully
   - [ ] Permission granted enables audio capture

4. **Audio Quality:**
   - [ ] Sample rate conversion works (48kHz → 16kHz)
   - [ ] No audio clipping or distortion
   - [ ] Stereo to mono conversion correct
   - [ ] Latency acceptable

5. **Edge Cases:**
   - [ ] No audio devices available
   - [ ] Permission revoked while recording
   - [ ] Multiple displays (which one to capture?)
   - [ ] App running without GUI (tray only)

### Phase 9: Documentation Updates

**Files to Update:**

1. **README.md**
   - Add ScreenCaptureKit section
   - Update macOS version requirements
   - Note that BlackHole is optional for macOS 13+

2. **CHANGELOG.md**
   - Document new ScreenCaptureKit support
   - Note breaking changes (if any)

3. **AGENTS.md**
   - Update architecture documentation
   - Add ScreenCaptureKit module info

4. **User-facing docs:**
   - Installation guide changes
   - Permission setup guide
   - Troubleshooting for both audio sources

### Phase 10: Gradual Rollout

**Strategy:**

1. **Beta Release (v1.x-beta):**
   - Feature flag to enable/disable ScreenCaptureKit
   - Collect user feedback
   - Monitor crash reports

2. **Default Enable (v1.x):**
   - Make ScreenCaptureKit default for macOS 13+
   - Keep BlackHole as fallback
   - Update onboarding flow

3. **Remove BlackHole Requirement (v2.0):**
   - Update minimum macOS requirement to 13.0
   - Remove BlackHole code paths
   - Simplify audio architecture

## Migration Checklist

### Backend (Rust)

- [ ] Add screencapturekit-rs dependency
- [ ] Create screencapturekit module
- [ ] Implement audio capture
- [ ] Add version detection
- [ ] Update AudioManager
- [ ] Add permission checking
- [ ] Implement resampling (48→16kHz)
- [ ] Test audio pipeline
- [ ] Add error handling
- [ ] Add logging

### Frontend (TypeScript/React)

- [ ] Add version detection UI
- [ ] Create audio source status component
- [ ] Update permission flow
- [ ] Update onboarding
- [ ] Add settings panel
- [ ] Add troubleshooting UI
- [ ] Update error messages

### Configuration

- [ ] Update Info.plist
- [ ] Update Entitlements.plist
- [ ] Update tauri.conf.json
- [ ] Add build flags

### Documentation

- [ ] README updates
- [ ] Implementation guide
- [ ] Migration plan
- [ ] User documentation
- [ ] CHANGELOG entry

### Testing

- [ ] Unit tests
- [ ] Integration tests
- [ ] Manual testing on macOS 13+
- [ ] Manual testing on macOS 12
- [ ] Permission flow testing
- [ ] Audio quality testing
- [ ] Performance testing

### Deployment

- [ ] Beta builds
- [ ] User feedback collection
- [ ] Bug fixes
- [ ] Stable release
- [ ] Documentation publication

## Timeline Estimate

- **Phase 1:** ✅ Completed
- **Phase 2-3:** 2-3 days (Rust implementation)
- **Phase 4-5:** 1-2 days (Integration)
- **Phase 6:** 1-2 days (Frontend)
- **Phase 7:** 1 day (Configuration)
- **Phase 8:** 2-3 days (Testing)
- **Phase 9:** 1 day (Documentation)
- **Phase 10:** Ongoing (Rollout)

**Total Estimated Time:** 2-3 weeks

## Success Metrics

1. **User Experience:**
   - Reduce setup complexity (no BlackHole installation for macOS 13+)
   - Maintain or improve audio quality
   - No increase in transcription errors

2. **Technical:**
   - Clean fallback mechanism
   - Minimal code duplication
   - Good error handling
   - Comprehensive logging

3. **Adoption:**
   - 80%+ of macOS 13+ users using ScreenCaptureKit
   - < 5% increase in permission-related support requests
   - No regression in user retention

## Risks & Mitigations

### Risk 1: screencapturekit-rs API Instability

**Mitigation:**
- Pin exact version in Cargo.toml
- Consider forking if needed
- Have BlackHole fallback always available

### Risk 2: Permission Complexity

**Mitigation:**
- Clear UI guidance
- Video tutorials
- Automated permission checking
- Helpful error messages

### Risk 3: Audio Quality Differences

**Mitigation:**
- Extensive A/B testing
- User feedback collection
- Allow manual source selection in settings

### Risk 4: Performance Issues

**Mitigation:**
- Profile CPU/memory usage
- Optimize buffer handling
- Benchmark against BlackHole

### Risk 5: macOS Version Detection Failures

**Mitigation:**
- Conservative fallback (use BlackHole if uncertain)
- Multiple detection methods
- User override in settings

## Rollback Plan

If critical issues arise:

1. **Immediate:** Feature flag to disable ScreenCaptureKit
2. **Short-term:** Revert to BlackHole-only in emergency patch
3. **Long-term:** Fix issues and re-enable gradually

## References

- [SCREENCAPTUREKIT_IMPLEMENTATION.md](./SCREENCAPTUREKIT_IMPLEMENTATION.md)
- [Azayaka Implementation](https://github.com/Mnpn/Azayaka)
- [screencapturekit-rs](https://github.com/doom-fish/screencapturekit-rs)
- [Apple ScreenCaptureKit Docs](https://developer.apple.com/documentation/screencapturekit)
