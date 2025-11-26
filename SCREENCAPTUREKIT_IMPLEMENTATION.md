# ScreenCaptureKit Implementation cho System Audio Capture

## Tổng Quan

ScreenCaptureKit là API mới của Apple (từ macOS 13 Ventura trở lên) cho phép capture màn hình và **system audio** mà không cần BlackHole hay các audio loopback device khác.

## Reference Implementation: Azayaka

Repository: [Mnpn/Azayaka](https://github.com/Mnpn/Azayaka)

Azayaka là một screen recorder sử dụng ScreenCaptureKit để capture cả video và system audio.

## Core Components

### 1. SCStreamConfiguration - Cấu hình Audio

```swift
var conf = SCStreamConfiguration()

// Enable audio capture từ system
conf.capturesAudio = true

// Cấu hình sample rate và channels
conf.sampleRate = 48000  // Hz
conf.channelCount = 2    // Stereo

// Có thể capture microphone riêng (macOS 15+)
if #available(macOS 15.0, *) {
    conf.captureMicrophone = true  // Optional: capture mic riêng
}

// Để capture audio-only, set kích thước video nhỏ
if audioOnly {
    conf.width = 2
    conf.height = 2
    conf.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale.max)
}
```

### 2. SCContentFilter - Chọn nguồn audio

```swift
// Option 1: Capture audio từ cả display (toàn bộ system audio)
let contentFilter = SCContentFilter(
    display: display,
    excludingApplications: [], 
    exceptingWindows: []
)

// Option 2: Capture audio từ một window cụ thể (chỉ audio của app đó)
let contentFilter = SCContentFilter(desktopIndependentWindow: window)
```

### 3. SCStream - Khởi tạo stream

```swift
let stream = SCStream(filter: contentFilter, configuration: conf, delegate: self)

// Add output handler cho audio
try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: .global())

// Optional: Add output cho microphone riêng (macOS 15+)
if #available(macOS 15.0, *), conf.captureMicrophone {
    try stream.addStreamOutput(self, type: .microphone, sampleHandlerQueue: .global())
}

// Start capture
try await stream.startCapture()
```

### 4. SCStreamOutput Protocol - Nhận audio data

```swift
extension AppDelegate: SCStreamOutput {
    func stream(_ stream: SCStream, 
                didOutputSampleBuffer sampleBuffer: CMSampleBuffer, 
                of outputType: SCStreamOutputType) {
        
        guard sampleBuffer.isValid else { return }
        
        switch outputType {
        case .audio:
            // Xử lý system audio
            processAudioBuffer(sampleBuffer)
            
        case .microphone:
            // Xử lý microphone audio (macOS 15+)
            processMicrophoneBuffer(sampleBuffer)
            
        case .screen:
            // Xử lý video frame (nếu có)
            break
            
        @unknown default:
            break
        }
    }
}
```

### 5. Xử lý Audio Buffer

```swift
func processAudioBuffer(_ sampleBuffer: CMSampleBuffer) {
    // Convert CMSampleBuffer sang AVAudioPCMBuffer nếu cần
    guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer) else { return }
    
    // Write trực tiếp vào file
    if let audioFile = audioFile {
        do {
            // Azayaka writes directly to AVAudioFile
            try audioFile.write(from: sampleBuffer)
        } catch {
            print("Error writing audio: \(error)")
        }
    }
}
```

### 6. Audio File Setup (AVAudioFile)

```swift
func prepareAudioRecording() throws {
    let audioSettings: [String: Any] = [
        AVSampleRateKey: 48000,
        AVNumberOfChannelsKey: 2,
        AVFormatIDKey: kAudioFormatMPEG4AAC,  // AAC
        AVEncoderBitRateKey: 128000  // 128kbps
    ]
    
    let filePath = getFilePath() + ".m4a"
    
    audioFile = try AVAudioFile(
        forWriting: URL(fileURLWithPath: filePath),
        settings: audioSettings,
        commonFormat: .pcmFormatFloat32,
        interleaved: false
    )
}
```

## Audio Format Options

Azayaka hỗ trợ nhiều format:

```swift
// AAC (lossy)
audioSettings[AVFormatIDKey] = kAudioFormatMPEG4AAC
audioSettings[AVEncoderBitRateKey] = 128000

// ALAC (lossless)
audioSettings[AVFormatIDKey] = kAudioFormatAppleLossless
audioSettings[AVEncoderBitDepthHintKey] = 16

// FLAC (lossless)
audioSettings[AVFormatIDKey] = kAudioFormatFLAC

// Opus (lossy, efficient)
audioSettings[AVFormatIDKey] = kAudioFormatOpus
audioSettings[AVEncoderBitRateKey] = 96000
```

## Permissions Required

Thêm vào `Info.plist`:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Azayaka needs microphone access to record audio</string>

<key>NSScreenCaptureUsageDescription</key>
<string>Azayaka needs screen recording permission to capture system audio</string>
```

Trong `Entitlements`:

```xml
<key>com.apple.security.device.audio-input</key>
<true/>
```

## Audio-Only Recording

Để chỉ record audio mà không cần video:

```swift
// Set resolution cực thấp
conf.width = 2
conf.height = 2

// Set frame interval cao nhất (ít frames nhất)
conf.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale.max)

// Vẫn cần enable audio
conf.capturesAudio = true
conf.sampleRate = 48000
conf.channelCount = 2
```

## Stopping Recording

```swift
func stopRecording() {
    // Stop stream
    stream?.stopCapture()
    stream = nil
    
    // Close audio file
    audioFile = nil  // This will flush and close the file
}
```

## macOS 15+ Features

### System Recorder (SCRecordingOutput)

macOS 15 có API mới cao cấp hơn:

```swift
if #available(macOS 15.0, *) {
    let output = SCRecordingOutputConfiguration()
    output.outputURL = URL(fileURLWithPath: filePath)
    output.outputFileType = .mp4
    output.videoCodecType = .hevc
    
    let recordingOutput = SCRecordingOutput(configuration: output, delegate: self)
    try stream?.addRecordingOutput(recordingOutput)
}
```

### Separate Microphone Stream

```swift
if #available(macOS 15.0, *) {
    conf.captureMicrophone = true
    try stream.addStreamOutput(self, type: .microphone, sampleHandlerQueue: .global())
}
```

## Key Advantages

1. **Không cần BlackHole**: Không cần cài đặt driver audio ảo
2. **Per-App Audio**: Có thể capture audio của một app cụ thể
3. **Microphone Riêng**: macOS 15+ tách biệt system audio và mic
4. **High Quality**: Capture trực tiếp từ system, không qua loopback
5. **Native Integration**: API chính thức của Apple

## Implementation cho Handy

### Kiến trúc đề xuất:

1. **Detect macOS Version**:
   ```rust
   #[cfg(target_os = "macos")]
   fn get_macos_version() -> (i32, i32) {
       // Get macOS version
   }
   
   fn should_use_screencapturekit() -> bool {
       let (major, minor) = get_macos_version();
       major >= 13  // macOS 13+
   }
   ```

2. **Swift Bridge cho ScreenCaptureKit**:
   - Tạo một Swift module riêng trong `src-tauri/bin/macos-screencapturekit/`
   - Expose C API để Rust gọi được
   - Hoặc sử dụng Objective-C bindings

3. **Fallback Strategy**:
   ```
   if macOS >= 13:
       use ScreenCaptureKit
   else:
       use BlackHole (current implementation)
   ```

## Example: Minimal Audio Capture

```swift
import ScreenCaptureKit
import AVFoundation

class AudioCapture: NSObject, SCStreamOutput, SCStreamDelegate {
    var stream: SCStream?
    var audioFile: AVAudioFile?
    
    func startCapture() async throws {
        // Get available content
        let content = try await SCShareableContent.current
        guard let display = content.displays.first else { return }
        
        // Configure for audio-only
        let config = SCStreamConfiguration()
        config.width = 2
        config.height = 2
        config.capturesAudio = true
        config.sampleRate = 48000
        config.channelCount = 2
        
        // Create filter
        let filter = SCContentFilter(display: display, 
                                      excludingApplications: [], 
                                      exceptingWindows: [])
        
        // Setup audio file
        let settings: [String: Any] = [
            AVSampleRateKey: 48000,
            AVNumberOfChannelsKey: 2,
            AVFormatIDKey: kAudioFormatMPEG4AAC
        ]
        audioFile = try AVAudioFile(forWriting: URL(fileURLWithPath: "/path/to/output.m4a"),
                                      settings: settings,
                                      commonFormat: .pcmFormatFloat32,
                                      interleaved: false)
        
        // Create and start stream
        stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream?.addStreamOutput(self, type: .audio, sampleHandlerQueue: .global())
        try await stream?.startCapture()
    }
    
    func stream(_ stream: SCStream, 
                didOutputSampleBuffer sampleBuffer: CMSampleBuffer, 
                of type: SCStreamOutputType) {
        if type == .audio {
            try? audioFile?.write(from: sampleBuffer)
        }
    }
    
    func stopCapture() {
        stream?.stopCapture()
        stream = nil
        audioFile = nil
    }
}
```

## Rust Implementation (screencapturekit-rs)

### Setup

Thêm dependency vào `Cargo.toml`:

```toml
[dependencies]
screencapturekit = "0.3"
core-media-rs = "0.1"
core-foundation = "0.9"
```

### Basic Audio Capture Example

```rust
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
use std::sync::mpsc::{channel, Sender};

struct AudioStreamOutput {
    sender: Sender<CMSampleBuffer>,
}

impl SCStreamOutputTrait for AudioStreamOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        if of_type == SCStreamOutputType::Audio {
            self.sender.send(sample_buffer).ok();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = channel();
    
    // Configure stream for audio capture
    let config = SCStreamConfiguration::new()
        .set_captures_audio(true)?
        .set_sample_rate(48000)?
        .set_channel_count(2)?;
    
    // Get display to capture from
    let display = SCShareableContent::get()?.displays().remove(0);
    
    // Create content filter
    let filter = SCContentFilter::new()
        .with_display_excluding_windows(&display, &[]);
    
    // Create stream
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(
        AudioStreamOutput { sender: tx },
        SCStreamOutputType::Audio
    );
    
    // Start capture
    stream.start_capture()?;
    
    // Process audio samples
    loop {
        let sample = rx.recv_timeout(std::time::Duration::from_secs(1))?;
        
        // Get audio buffer
        let audio_buffer_list = sample.get_audio_buffer_list()?;
        
        for i in 0..audio_buffer_list.num_buffers() {
            let buffer = audio_buffer_list.get(i)?;
            let audio_data = buffer.data();
            
            // Process audio data here
            // e.g., write to file, send to Whisper, etc.
            println!("Got {} bytes of audio", audio_data.len());
        }
    }
    
    // Stop capture
    stream.stop_capture()?;
    Ok(())
}
```

### Integration với Handy's Audio Pipeline

```rust
// src-tauri/src/audio_toolkit/screencapturekit/mod.rs

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
use rubato::{Resampler, SincFixedIn, InterpolationType, InterpolationParameters, WindowFunction};

pub struct ScreenCaptureKitAudio {
    stream: Option<SCStream>,
    resampler: Option<SincFixedIn<f32>>,
    audio_sender: crossbeam_channel::Sender<Vec<f32>>,
}

impl ScreenCaptureKitAudio {
    pub fn new(audio_sender: crossbeam_channel::Sender<Vec<f32>>) -> Self {
        // Resampler: 48kHz (ScreenCaptureKit) -> 16kHz (Whisper)
        let params = InterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: InterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        
        let resampler = SincFixedIn::<f32>::new(
            16000.0 / 48000.0,  // ratio
            2.0,                 // max_relative_ratio
            params,
            1024,                // chunk_size
            2,                   // channels
        ).ok();
        
        Self {
            stream: None,
            resampler,
            audio_sender,
        }
    }
    
    pub fn start_capture(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Configure for audio capture
        let config = SCStreamConfiguration::new()
            .set_captures_audio(true)?
            .set_sample_rate(48000)?
            .set_channel_count(2)?;
        
        // Get display
        let display = SCShareableContent::get()?.displays().remove(0);
        
        // Create filter - capture all system audio
        let filter = SCContentFilter::new()
            .with_display_excluding_windows(&display, &[]);
        
        // Create stream with output handler
        let sender = self.audio_sender.clone();
        let mut stream = SCStream::new(&filter, &config);
        
        stream.add_output_handler(
            ScreenCaptureKitOutput { sender },
            SCStreamOutputType::Audio
        );
        
        stream.start_capture()?;
        self.stream = Some(stream);
        
        Ok(())
    }
    
    pub fn stop_capture(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stream) = self.stream.take() {
            stream.stop_capture()?;
        }
        Ok(())
    }
}

struct ScreenCaptureKitOutput {
    sender: crossbeam_channel::Sender<Vec<f32>>,
}

impl SCStreamOutputTrait for ScreenCaptureKitOutput {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        if of_type != SCStreamOutputType::Audio {
            return;
        }
        
        // Extract audio data
        if let Ok(audio_buffer_list) = sample_buffer.get_audio_buffer_list() {
            for i in 0..audio_buffer_list.num_buffers() {
                if let Ok(buffer) = audio_buffer_list.get(i) {
                    let data = buffer.data();
                    
                    // Convert to f32 samples
                    let samples: Vec<f32> = data
                        .chunks_exact(4)
                        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();
                    
                    // Send to audio pipeline
                    self.sender.send(samples).ok();
                }
            }
        }
    }
}
```

### Conditional Compilation

```rust
// src-tauri/src/managers/audio.rs

#[cfg(target_os = "macos")]
fn get_macos_version() -> (u32, u32) {
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
pub fn should_use_screencapturekit() -> bool {
    if let Some((major, _minor)) = get_macos_version() {
        major >= 13  // macOS Ventura or later
    } else {
        false
    }
}

pub enum SystemAudioCapture {
    #[cfg(target_os = "macos")]
    ScreenCaptureKit(ScreenCaptureKitAudio),
    
    #[cfg(target_os = "macos")]
    BlackHole(BlackHoleAudio),
    
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}

impl AudioManager {
    pub fn new() -> Self {
        let capture = if cfg!(target_os = "macos") && should_use_screencapturekit() {
            SystemAudioCapture::ScreenCaptureKit(ScreenCaptureKitAudio::new(sender))
        } else if cfg!(target_os = "macos") {
            SystemAudioCapture::BlackHole(BlackHoleAudio::new(sender))
        } else {
            SystemAudioCapture::Unsupported
        };
        
        Self { capture, /* ... */ }
    }
}
```

## Resources

- [Apple ScreenCaptureKit Documentation](https://developer.apple.com/documentation/screencapturekit)
- [Azayaka Source Code](https://github.com/Mnpn/Azayaka) - Swift implementation
- [screencapturekit-rs](https://github.com/doom-fish/screencapturekit-rs) - Rust bindings
- [WWDC 2022: Meet ScreenCaptureKit](https://developer.apple.com/videos/play/wwdc2022/10156/)

## Notes

- ScreenCaptureKit requires Screen Recording permission
- Audio capture works even with muted system volume
- Can capture audio from specific apps or entire system
- macOS 15+ has improved API with SCRecordingOutput
- Azayaka's implementation is production-ready and well-tested
- `screencapturekit-rs` provides Rust bindings but may need updates for latest features
- Consider fallback to BlackHole for macOS < 13
