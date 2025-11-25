# 🏗️ Kiến Trúc và Luồng Hoạt Động của System Audio Capture

## 📋 Tổng Quan

Hệ thống capture system audio trên macOS sử dụng **BlackHole virtual audio device** kết hợp với **Multi-Output Device** để vừa phát audio ra speakers vừa capture audio cho transcription.

---

## 🔄 Luồng Hoạt Động Từ Đầu Đến Cuối

### 1. **Khởi Tạo (Initialization)**

```
App Start
  ↓
AudioRecordingManager::start_microphone_stream()
  ↓
MacOSSystemAudio::new() → Tạo struct với:
  - sample_buffer: Arc<Mutex<VecDeque<f32>>> (thread-safe buffer)
  - is_capturing: false
  - use_blackhole: false
```

**Công nghệ:**
- **Rust**: Struct với `Arc<Mutex<>>` cho thread-safe shared state
- **Tauri**: `AppHandle` để emit events đến frontend

---

### 2. **Device Detection (Phát Hiện Thiết Bị)**

```
start_capture()
  ↓
find_blackhole_device()
  ↓
cpal::default_host()
  ↓
Enumerate input devices → Tìm "BlackHole 2ch"
  ↓
Check default OUTPUT device:
  - Nếu là "BlackHole 2ch" → ✅ Audio sẽ được route
  - Nếu là "Multi-Output Device" → ✅ Audio route đến cả speakers + BlackHole
  - Nếu là speakers khác → ❌ Warning: Audio không route đến BlackHole
```

**Công nghệ:**
- **cpal (Cross-Platform Audio Library)**: Enumerate audio devices
- **macOS CoreAudio**: Underlying API để query devices

**Code:**
```rust
let host = crate::audio_toolkit::get_cpal_host();
if let Ok(devices) = host.input_devices() {
    for device in devices {
        if device.name()?.contains("BlackHole") {
            return Some(device);
        }
    }
}
```

---

### 3. **Audio Capture (Bắt Đầu Capture)**

```
start_blackhole_capture(device)
  ↓
device.default_input_config() → Lấy config (48kHz, stereo)
  ↓
Spawn worker thread:
  ├─ build_blackhole_stream_in_thread()
  │   └─ device.build_input_stream() → Tạo cpal::Stream
  │       └─ stream.play() → Bắt đầu capture
  │
  └─ Stream callback được gọi mỗi khi có audio:
      └─ data: &[T] → Convert to f32 → Push vào sample_buffer
```

**Công nghệ:**
- **cpal::Stream**: Real-time audio stream với callback-based API
- **Thread-based architecture**: Stream chạy trong worker thread để tránh `Send` issues
- **Arc<Mutex<VecDeque<f32>>>**: Thread-safe circular buffer

**Code:**
```rust
let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
    let mut buf = buffer.lock().unwrap();
    // Convert to mono và push vào buffer
    for frame in data.chunks_exact(channels) {
        let mono = frame.iter().sum::<f32>() / channels as f32;
        buf.push_back(mono);
    }
};
```

**Luồng Audio:**
```
Chrome/App phát audio
  ↓
macOS Audio System
  ↓
Multi-Output Device (user đã cấu hình)
  ├─ Route đến "Mac mini Speakers" → 🔊 User nghe được
  └─ Route đến "BlackHole 2ch" → 📥 App capture được
      ↓
cpal::Stream callback
      ↓
sample_buffer (Arc<Mutex<VecDeque<f32>>>)
```

---

### 4. **Audio Processing Pipeline (Xử Lý Audio)**

```
Auto-transcription Thread (chạy mỗi 3 giây)
  ↓
read_samples() → Đọc từ sample_buffer
  ↓
FrameResampler::push()
  ├─ Input: 48kHz samples (từ BlackHole)
  ├─ Process: FFT-based resampling (rubato library)
  └─ Output: 16kHz samples (cho Whisper/Parakeet)
  ↓
accumulated_buffer (VecDeque<f32>)
  ├─ Sliding window: Giữ 4s audio (64,000 samples)
  ├─ Overlap: Giữ 1s overlap để không mất audio
  └─ Minimum: Cần 2s (32,000 samples) trước khi transcribe
```

**Công nghệ:**
- **rubato**: FFT-based audio resampling library (high quality)
- **FrameResampler**: Wrapper để resample từng chunk (1024 samples)
- **Sliding window**: Giống Google Translate - không mất audio giữa các lần transcribe

**Code:**
```rust
const SYSTEM_AUDIO_SAMPLE_RATE: usize = 48000;  // BlackHole output
const TARGET_SAMPLE_RATE: usize = 16000;        // Whisper/Parakeet input

let mut resampler = FrameResampler::new(
    SYSTEM_AUDIO_SAMPLE_RATE,
    TARGET_SAMPLE_RATE,
    Duration::from_millis(30),
);

// Resample từng chunk
resampler.push(&new_samples, |chunk| {
    accumulated_buffer.extend_from_slice(chunk);
});
```

**Tại sao cần resample?**
- BlackHole capture ở **48kHz** (standard audio quality)
- Whisper/Parakeet models yêu cầu **16kHz** (speech recognition standard)
- Resampling giảm data size: 48kHz → 16kHz = giảm 3x

---

### 5. **Audio Quality Detection (Phát Hiện Chất Lượng Audio)**

```
Mỗi lần process samples:
  ↓
Calculate RMS (Root Mean Square):
  RMS = sqrt(sum(sample²) / count)
  ↓
Calculate Max Amplitude:
  Max = max(|sample|)
  ↓
Detect transition:
  - previous_rms < 0.00001 (silence)
  - current_rms > 0.00001 (audio detected)
  ↓
✅ Emit event: "AUDIO DETECTED!"
```

**Công nghệ:**
- **RMS (Root Mean Square)**: Measure audio energy/amplitude
- **Threshold-based detection**: 0.00001 = very low threshold để detect bất kỳ audio nào

**Code:**
```rust
let rms = (samples.iter().map(|&s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
let max_amp = samples.iter().map(|&s| s.abs()).fold(0.0f32, |a, b| a.max(b));

if was_silent && is_now_audio {
    log::info!("🎉 AUDIO DETECTED! RMS: {:.6}", rms);
    app_handle.emit("log-update", "✅✅✅ AUDIO DETECTED!");
}
```

---

### 6. **Transcription (Chuyển Đổi Speech → Text)**

```
accumulated_buffer có đủ samples (≥ 32,000 = 2s)
  ↓
Take samples (giữ 1s overlap):
  - Take: accumulated_buffer.len() - OVERLAP_SAMPLES
  - Keep: OVERLAP_SAMPLES (1s) cho lần transcribe tiếp theo
  ↓
TranscriptionManager::transcribe(samples)
  ├─ Ensure model loaded (Parakeet V3)
  ├─ Convert Vec<f32> → Audio vector
  └─ Run inference
  ↓
Result: String (transcription text)
  ↓
Emit events:
  ├─ "live-caption-update" → Frontend hiển thị live caption
  ├─ "log-update" → Log transcription result
  └─ paste() → Paste vào active app
```

**Công nghệ:**
- **Parakeet V3**: OnnxRuntime-based speech-to-text model
- **Sliding window transcription**: Mỗi 3 giây transcribe 3s audio, giữ 1s overlap
- **Tauri Events**: Real-time communication backend ↔ frontend

**Code:**
```rust
// Take samples (keep overlap)
let samples_to_transcribe: Vec<f32> = 
    accumulated_buffer.drain(..take_count).collect();

// Transcribe
match tm.transcribe(samples_to_transcribe) {
    Ok(transcription) => {
        // Emit to frontend
        app_handle.emit("live-caption-update", transcription)?;
        // Paste to active app
        paste(transcription, app_handle)?;
    }
}
```

**Tại sao sliding window?**
- **Real-time**: Transcribe mỗi 3s thay vì đợi hết audio
- **No audio loss**: Overlap 1s đảm bảo không mất audio ở ranh giới
- **Low latency**: User thấy caption ngay, không đợi hết video

---

## 🛠️ Công Nghệ và Thư Viện

### **Backend (Rust)**

1. **cpal (Cross-Platform Audio Library)**
   - **Mục đích**: Enumerate devices, tạo audio streams
   - **API**: `DeviceTrait`, `StreamTrait`, `HostTrait`
   - **Platform**: macOS CoreAudio, Windows WASAPI, Linux ALSA

2. **rubato**
   - **Mục đích**: High-quality audio resampling
   - **Algorithm**: FFT-based (Fast Fourier Transform)
   - **Performance**: Real-time, low latency

3. **Tauri**
   - **Mục đích**: Desktop app framework (Rust + Web frontend)
   - **Events**: `app_handle.emit()` để gửi data đến frontend
   - **Commands**: Frontend gọi backend functions

4. **Arc<Mutex<>>**
   - **Mục đích**: Thread-safe shared state
   - **Arc**: Atomic Reference Counting (shared ownership)
   - **Mutex**: Mutual exclusion (thread-safe access)

### **Frontend (React/TypeScript)**

1. **React Hooks**
   - `useEffect`: Listen to Tauri events
   - `useState`: Manage UI state

2. **Tauri API**
   - `listen()`: Listen to backend events
   - `invoke()`: Call backend commands

---

## 📊 Kiến Trúc Thread

```
Main Thread (Tauri)
  ├─ UI Event Loop
  └─ Command Handlers

BlackHole Capture Thread
  ├─ cpal::Stream callback
  └─ Push samples → sample_buffer (Arc<Mutex<>>)

Auto-Transcription Thread
  ├─ Loop every 3 seconds
  ├─ Read from sample_buffer
  ├─ Resample 48kHz → 16kHz
  ├─ Accumulate in buffer
  └─ Transcribe when ready

Transcription Thread (OnnxRuntime)
  └─ Model inference (blocking)
```

**Thread Safety:**
- `sample_buffer`: `Arc<Mutex<VecDeque<f32>>>` - shared giữa capture thread và transcription thread
- `blackhole_thread`: `JoinHandle` - quản lý lifecycle của capture thread
- `blackhole_stop_tx`: `mpsc::Sender` - signal để stop capture thread

---

## 🔍 Chi Tiết Kỹ Thuật

### **1. BlackHole Virtual Audio Device**

**Là gì?**
- Virtual audio driver cho macOS
- Tạo một "virtual cable" để route audio

**Cách hoạt động:**
```
App phát audio → macOS Audio System
  ↓
Multi-Output Device (user cấu hình)
  ├─ Output 1: Real speakers → 🔊 User nghe
  └─ Output 2: BlackHole → 📥 Virtual device
      ↓
BlackHole exposes như input device
      ↓
cpal capture từ BlackHole như microphone
```

**Tại sao dùng BlackHole?**
- ✅ Không cần Screen Recording permission
- ✅ Hoạt động ổn định hơn ScreenCaptureKit
- ✅ User có thể nghe audio (với Multi-Output Device)

### **2. FrameResampler (48kHz → 16kHz)**

**Algorithm:**
- **FFT-based resampling** (rubato library)
- **Chunk-based processing**: Xử lý từng chunk 1024 samples
- **Frame-based output**: Emit theo frame duration (30ms)

**Tại sao 16kHz?**
- Whisper/Parakeet models được train ở 16kHz
- Speech recognition không cần full audio quality
- Giảm computation: 48kHz → 16kHz = giảm 3x samples

### **3. Sliding Window Transcription**

**Strategy:**
```
Time:  0s    3s    6s    9s    12s
       |-----|-----|-----|-----|
       [Chunk 1] (3s)
            [Chunk 2] (3s, overlap 1s)
                 [Chunk 3] (3s, overlap 1s)
```

**Benefits:**
- ✅ Real-time: User thấy caption ngay
- ✅ No audio loss: Overlap đảm bảo không mất audio
- ✅ Low latency: 3s delay thay vì đợi hết audio

---

## 🎯 Tóm Tắt Luồng Hoạt Động

**⚠️ QUAN TRỌNG:** Hiện tại app **CHỈ dùng BlackHole** để capture system audio. ScreenCaptureKit chỉ là fallback và **KHÔNG được sử dụng** trong trường hợp bình thường.

```
1. User cấu hình Multi-Output Device (BlackHole + Speakers)
   ↓
2. App start → Detect BlackHole device
   ↓
3. ✅ BlackHole được tìm thấy → Start capture từ BlackHole (48kHz)
   ❌ BlackHole KHÔNG được tìm thấy → Fallback ScreenCaptureKit (chỉ khi BlackHole không có)
   ↓
4. Audio callback từ BlackHole → Push vào sample_buffer
   ↓
5. Auto-transcription thread (mỗi 3s):
   - Read từ sample_buffer
   - Resample 48kHz → 16kHz
   - Accumulate trong buffer (4s, overlap 1s)
   - Transcribe khi đủ 2s audio
   ↓
6. Emit "live-caption-update" → Frontend hiển thị
   ↓
7. Paste transcription vào active app
```

**Lưu ý về Fallback:**
- ScreenCaptureKit **CHỈ** được dùng khi:
  - BlackHole device không được tìm thấy (user chưa cài BlackHole)
  - BlackHole start failed với error
- ScreenCaptureKit **KHÔNG** được dùng khi:
  - BlackHole được tìm thấy nhưng không có audio (user chưa cấu hình Multi-Output Device)
  - Trong trường hợp này, app giữ BlackHole running và monitor cho audio

---

## 🔧 Các Thành Phần Chính

### **MacOSSystemAudio**
- Quản lý BlackHole capture (primary method)
- Fallback ScreenCaptureKit (chỉ khi BlackHole không có sẵn)
- Thread-safe sample buffer
- Device detection và monitoring
- **Hiện tại:** Chỉ dùng BlackHole trong production (ScreenCaptureKit là fallback không được test)

### **FrameResampler**
- Resample 48kHz → 16kHz
- Chunk-based processing
- Frame-based output

### **AudioRecordingManager**
- Quản lý transcription loop
- Sliding window accumulation
- Event emission

### **TranscriptionManager**
- Model loading và inference
- Parakeet V3 integration
- Result processing

---

## 📈 Performance Characteristics

- **Latency**: ~3-4 giây (transcription interval + processing)
- **CPU Usage**: Moderate (resampling + inference)
- **Memory**: ~50MB (model + buffers)
- **Audio Quality**: 48kHz capture → 16kHz transcription (đủ cho speech)

---

## 🖥️ macOS Version Support

### **Minimum System Version**

**App Bundle:** macOS 10.13 (High Sierra) - theo `tauri.conf.json`
```json
"macOS": {
  "minimumSystemVersion": "10.13"
}
```

### **System Audio Capture Methods**

**1. BlackHole (Primary Method - Khuyến nghị) ⭐**
- **Supported macOS:** 10.9+ (Mavericks trở lên)
- **Không cần version check** - BlackHole hoạt động trên macOS cũ
- **Không cần Screen Recording permission** - chỉ cần Microphone permission
- **Hoạt động ổn định** trên mọi macOS version hỗ trợ BlackHole

**2. ScreenCaptureKit (Fallback Method)**
- **Required macOS:** 13.0+ (Ventura trở lên)
- **Code check:**
  ```swift
  if #available(macOS 13.0, *) {
      runCapture()
  } else {
      log("macOS 13.0 or later required")
      exit(1)
  }
  ```
- **Cần Screen Recording permission**
- **Có thể không hoạt động** ngay cả khi có permission (hạn chế của macOS)

### **Kết Luận về Version Support**

**✅ Hỗ trợ tốt nhất:**
- **macOS 13+ (Ventura)**: Cả BlackHole và ScreenCaptureKit đều available
- **macOS 10.9 - 12.x**: Chỉ BlackHole (ScreenCaptureKit không available)

**⚠️ Hạn chế:**
- **macOS < 10.9**: Không hỗ trợ (quá cũ, không có trong minimum requirements)
- **macOS 10.13 - 12.x**: Cần cài BlackHole để dùng system audio capture

**📋 Khuyến nghị:**
- **Nếu macOS 13+**: App sẽ tự động thử BlackHole trước, fallback ScreenCaptureKit nếu BlackHole không có
- **Nếu macOS < 13**: Chỉ dùng BlackHole (ScreenCaptureKit không available)
- **Tốt nhất**: Cài BlackHole trên mọi macOS version để có trải nghiệm tốt nhất

---

## 🎓 Kết Luận

Hệ thống sử dụng:
- **BlackHole** để capture system audio
- **Multi-Output Device** để vừa nghe vừa capture
- **cpal** để interface với audio system
- **rubato** để resample audio
- **Sliding window** để real-time transcription
- **Thread-safe architecture** để xử lý concurrent audio streams

Đây là một kiến trúc **production-ready**, **scalable**, và **user-friendly** cho system audio capture trên macOS! 🚀

