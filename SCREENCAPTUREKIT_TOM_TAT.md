# Tóm Tắt: ScreenCaptureKit cho Handy

## Vấn Đề Hiện Tại

Handy đang dùng **BlackHole** để capture system audio trên macOS. Nhưng BlackHole:
- Yêu cầu người dùng phải cài đặt thủ công
- Cần cấu hình Multi-Output Device phức tạp
- Gây khó khăn cho người dùng mới

## Giải Pháp: ScreenCaptureKit

**ScreenCaptureKit** là API chính thức của Apple (từ macOS 13 Ventura) cho phép:
- ✅ Capture system audio **trực tiếp** mà không cần BlackHole
- ✅ Native API, không cần cài thêm driver
- ✅ Capture audio của từng app riêng biệt (nếu muốn)
- ✅ Chất lượng cao, ít CPU overhead

## Tham Khảo Implementation

### 1. Azayaka (Swift - macOS)
Repository: https://github.com/Mnpn/Azayaka

**Code quan trọng:**

```swift
// Configure stream
var conf = SCStreamConfiguration()
conf.capturesAudio = true       // Bật audio capture
conf.sampleRate = 48000          // 48kHz
conf.channelCount = 2            // Stereo

// Create filter để chọn nguồn audio
let contentFilter = SCContentFilter(
    display: display,
    excludingApplications: [],
    exceptingWindows: []
)

// Tạo stream
let stream = SCStream(filter: contentFilter, configuration: conf, delegate: self)
try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: .global())
try await stream.startCapture()
```

**Xử lý audio buffer:**

```swift
func stream(_ stream: SCStream, 
            didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
            of outputType: SCStreamOutputType) {
    if outputType == .audio {
        // Write vào file hoặc process
        try? audioFile.write(from: sampleBuffer)
    }
}
```

### 2. screencapturekit-rs (Rust Bindings)
Repository: https://github.com/doom-fish/screencapturekit-rs

**Rust example:**

```rust
use screencapturekit::*;

// Configure
let config = SCStreamConfiguration::new()
    .set_captures_audio(true)?
    .set_sample_rate(48000)?
    .set_channel_count(2)?;

// Get display
let display = SCShareableContent::get()?.displays().remove(0);

// Create filter
let filter = SCContentFilter::new()
    .with_display_excluding_windows(&display, &[]);

// Create stream
let mut stream = SCStream::new(&filter, &config);
stream.add_output_handler(AudioHandler, SCStreamOutputType::Audio);
stream.start_capture()?;
```

## Kiến Trúc Đề Xuất cho Handy

```
┌─────────────────────────────────────┐
│   User Input (Keyboard Shortcut)   │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│        Audio Manager               │
│                                     │
│  ┌─ Version Check: macOS >= 13? ─┐│
│  │                                 ││
│  │  YES → ScreenCaptureKit        ││
│  │  NO  → BlackHole (fallback)    ││
│  └─────────────────────────────────┘│
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│     Audio Samples (48kHz, Stereo)   │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│    Resampler (48kHz → 16kHz Mono)   │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│   VAD (Voice Activity Detection)    │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│      Whisper Transcription          │
└─────────────────┬───────────────────┘
                  │
┌─────────────────▼───────────────────┐
│        Clipboard Output             │
└─────────────────────────────────────┘
```

## Các Bước Implement

### Bước 1: Thêm Rust Bindings

```toml
# Cargo.toml
[target.'cfg(target_os = "macos")'.dependencies]
screencapturekit = "0.3"
core-media-rs = "0.1"
```

### Bước 2: Tạo Module mới

```
src-tauri/src/audio_toolkit/screencapturekit/
├── mod.rs
├── capture.rs      # Logic capture audio
└── permissions.rs  # Check permissions
```

### Bước 3: Detect macOS Version

```rust
fn should_use_screencapturekit() -> bool {
    let (major, _) = get_macos_version();
    major >= 13  // macOS Ventura+
}
```

### Bước 4: Integration

```rust
pub enum SystemAudioSource {
    ScreenCaptureKit(ScreenCaptureKitAudio),
    BlackHole(BlackHoleAudio),
}

impl AudioManager {
    pub fn new() -> Self {
        let source = if should_use_screencapturekit() {
            SystemAudioSource::ScreenCaptureKit(...)
        } else {
            SystemAudioSource::BlackHole(...)
        };
        
        Self { source, ... }
    }
}
```

## Permissions Cần Thiết

### Info.plist
```xml
<key>NSScreenCaptureUsageDescription</key>
<string>Handy needs screen recording permission to capture system audio</string>
```

### Entitlements.plist
```xml
<key>com.apple.security.device.audio-input</key>
<true/>
```

## Ưu Điểm

1. **Dễ dàng hơn cho người dùng:**
   - macOS 13+: Không cần cài BlackHole
   - Chỉ cần grant permission "Screen Recording"
   - Tự động fallback về BlackHole nếu macOS cũ

2. **Chất lượng tốt hơn:**
   - Capture trực tiếp từ system
   - Không qua audio loopback device
   - Ít latency hơn

3. **Linh hoạt hơn:**
   - Có thể capture audio từ app cụ thể
   - Có thể capture microphone riêng (macOS 15+)
   - Native API, ổn định lâu dài

## Lưu Ý Quan Trọng

### 1. Sample Rate
- ScreenCaptureKit output: **48kHz**
- Whisper cần: **16kHz**
- ⚠️ Phải resample: 48kHz → 16kHz

### 2. Channels
- ScreenCaptureKit output: **Stereo (2 channels)**
- Whisper cần: **Mono (1 channel)**
- ⚠️ Phải convert: Stereo → Mono

### 3. Format
- ScreenCaptureKit output: **Float32 PCM**
- Whisper cần: **Float32 PCM** ✅ (giống nhau)

### 4. Permissions
- Cần permission "Screen Recording" (mới)
- Khác với "Microphone" permission (đã có)
- User phải grant qua System Preferences

## Testing Plan

### Cần test trên:
- ✅ macOS 13 Ventura
- ✅ macOS 14 Sonoma  
- ✅ macOS 15 Sequoia
- ✅ macOS 12 Monterey (fallback to BlackHole)

### Test cases:
- Audio capture hoạt động không cần BlackHole
- Permission flow hoạt động đúng
- Audio quality tương đương BlackHole
- VAD vẫn hoạt động chính xác
- Transcription accuracy không giảm
- Fallback về BlackHole nếu macOS < 13

## Timeline Ước Tính

- **Research & Documentation:** ✅ Hoàn thành
- **Rust Implementation:** 2-3 ngày
- **Integration & Testing:** 2-3 ngày
- **UI Updates:** 1-2 ngày
- **Documentation:** 1 ngày
- **Beta Testing:** 1 tuần

**Tổng:** ~2-3 tuần

## Files Đã Tạo

1. ✅ `SCREENCAPTUREKIT_IMPLEMENTATION.md` - Chi tiết technical implementation
2. ✅ `SCREENCAPTUREKIT_MIGRATION_PLAN.md` - Kế hoạch migration đầy đủ
3. ✅ `SCREENCAPTUREKIT_TOM_TAT.md` - File này (tóm tắt tiếng Việt)

## Next Steps

1. Review các file documentation
2. Quyết định có implement không
3. Nếu có, bắt đầu từ Phase 2 trong Migration Plan
4. Test kỹ trên nhiều macOS versions
5. Beta release để user test

## Resources

- [Apple Docs](https://developer.apple.com/documentation/screencapturekit)
- [Azayaka Code](https://github.com/Mnpn/Azayaka)
- [Rust Bindings](https://github.com/doom-fish/screencapturekit-rs)
- [WWDC Video](https://developer.apple.com/videos/play/wwdc2022/10156/)
