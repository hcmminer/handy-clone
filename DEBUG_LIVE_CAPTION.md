# Tổng Kết: Debug Live Caption - System Audio Transcription

## 📋 Tổng Quan Vấn Đề

**Vấn đề ban đầu:** App không hiển thị live caption mặc dù Chrome đang phát video với audio.

**Mục tiêu:** Implement real-time transcription với live captioning giống Google Translate, không bỏ sót bất kỳ lời thoại nào.

---

## 🔍 Nguyên Nhân Gốc Rễ

### 1. **Vấn đề Sample Rate Mismatch** (Nguyên nhân chính)
- **Vấn đề:** System audio từ ScreenCaptureKit (SCK) là **48kHz**, nhưng Whisper model cần **16kHz**
- **Triệu chứng:** Transcription trả về empty string (`len=0`) mặc dù đã nhận được audio samples
- **Giải pháp:** Thêm `FrameResampler` để resample từ 48kHz → 16kHz trước khi gửi đến transcription

### 2. **Vấn đề Display Capture không gửi Audio Buffers**
- **Vấn đề:** SCStream với `SCContentFilter(display: display, excludingWindows: [])` không gọi delegate callbacks
- **Triệu chứng:** `bufferCount: 0, nonAudioCount: 0` - SCStream không gọi delegate
- **Giải pháp:** Đổi sang capture từ **ALL applications** thay vì display:
  ```swift
  filter = SCContentFilter(display: display, including: shareableApps, exceptingWindows: [])
  ```

### 3. **Vấn đề Log không đủ chi tiết**
- **Vấn đề:** Không biết được transcription pipeline đang dừng ở bước nào
- **Giải pháp:** Thêm log chi tiết ở mỗi bước:
  - Resampler initialization
  - Sample reading và resampling
  - Buffer accumulation
  - Transcription start/completion
  - Live caption event emission

---

## 🛠️ Các Bước Debug và Fix

### Bước 1: Thêm Debug Logs cho SCK Helper
**File:** `src-tauri/src/audio_toolkit/macos_audio_capture.swift`

- Thêm log cho `streamDidStart` delegate method
- Thêm log cho `bufferCount` và `nonAudioCount` để biết SCStream có gọi delegate không
- Thêm log cho stream configuration và content filter

**Kết quả:** Phát hiện SCStream không gọi delegate với display capture

### Bước 2: Đổi Strategy Capture
**File:** `src-tauri/src/audio_toolkit/macos_audio_capture.swift`

**Trước:**
```swift
filter = SCContentFilter(display: display, excludingWindows: [])
```

**Sau:**
```swift
filter = SCContentFilter(display: display, including: shareableApps, exceptingWindows: [])
```

**Kết quả:** Nhận được "✅ First audio buffer received!"

### Bước 3: Thêm Resampler
**File:** `src-tauri/src/managers/audio.rs`

- Import `FrameResampler` từ `audio_toolkit::audio`
- Khởi tạo resampler: `48000kHz → 16000kHz`
- Resample samples trước khi thêm vào `accumulated_buffer`

**Code:**
```rust
const SYSTEM_AUDIO_SAMPLE_RATE: usize = 48000;
const TARGET_SAMPLE_RATE: usize = 16000;
let mut resampler = FrameResampler::new(
    SYSTEM_AUDIO_SAMPLE_RATE,
    TARGET_SAMPLE_RATE,
    Duration::from_millis(30),
);

// Trong loop:
let mut resampled_samples = Vec::new();
resampler.push(&new_samples, |chunk| {
    resampled_samples.extend_from_slice(chunk);
});
accumulated_buffer.extend(resampled_samples);
```

**Kết quả:** Transcription bắt đầu trả về text thay vì empty string

### Bước 4: Thêm Log Chi Tiết
**File:** `src-tauri/src/managers/audio.rs`

Thêm log ở các điểm quan trọng:
- `📊 [Auto-transcription] Resampler initialized`
- `📥 [Auto-transcription] Resampled X samples (48kHz) -> Y samples (16kHz)`
- `✅ [Auto-transcription] Buffer has X samples, ready to transcribe!`
- `🔄 [Auto-transcription] Starting transcription`
- `📝 [Auto-transcription] Raw transcription received`
- `🎯 [Auto-transcription] Result`
- `📤 [LiveCaption] Emitting event`
- `✅ [LiveCaption] Event emitted successfully`

**Kết quả:** Có thể trace được toàn bộ pipeline từ audio → transcription → live caption

---

## 📊 Cách Xem Log Real-Time

### 1. **Xem Log trong App (Recommended)**
- Mở app → Settings → General
- Scroll xuống phần **"Real-Time Logs"**
- Xem log real-time với filter và search
- Có nút **"Copy Logs"** để copy toàn bộ logs

### 2. **Xem Log từ Terminal (macOS)**

#### Xem toàn bộ log:
```bash
tail -f ~/Library/Logs/com.pais.handy/handy.log
```

#### Xem log với filter (Live Caption):
```bash
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(LiveCaption|Result|Raw transcription)"
```

#### Xem log với filter (Audio Capture):
```bash
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(SCK Helper|System capture read|First audio)"
```

#### Xem log với filter (Transcription Pipeline):
```bash
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(Resampled|Starting transcription|Processing samples)"
```

#### Xem log mới nhất (100 dòng):
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log
```

### 3. **Xem Log từ Dev Console (Frontend)**
- Mở DevTools (Cmd+Option+I)
- Tab Console
- Filter: `[LiveCaption]` hoặc `🎯`

---

## 🔑 Các Log Quan Trọng Cần Theo Dõi

### 1. **Permission & Capture Status**
```
✅ PERMISSION GRANTED!
✅ Capture started successfully
✅ First audio buffer received!
```
**Nếu không thấy:** Kiểm tra Screen Recording permission trong System Settings

### 2. **Audio Samples**
```
✅ System capture read X samples from buffer
📥 [Auto-transcription] Resampled X samples (48kHz) -> Y samples (16kHz)
```
**Nếu không thấy:** SCK helper không nhận được audio hoặc không gửi về Rust

### 3. **Transcription Pipeline**
```
🔄 [Auto-transcription] Starting transcription for X samples
📝 [Auto-transcription] Raw transcription received (len=X): '...'
🎯 [Auto-transcription] Result (len=X): '...'
```
**Nếu `len=0`:** Có thể do:
- Sample rate không đúng (cần resample)
- Audio không có speech (chỉ có music/noise)
- Model chưa load

### 4. **Live Caption Events**
```
📤 [LiveCaption] Emitting event with caption (X chars): '...'
✅ [LiveCaption] Event emitted successfully
```
**Nếu không thấy:** Event không được emit hoặc frontend không nhận được

---

## 🐛 Debug Checklist

Khi live caption không hoạt động, check theo thứ tự:

### ✅ Step 1: Permission
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "PERMISSION"
```
- Phải thấy: `✅ PERMISSION GRANTED!`
- Nếu không: Grant Screen Recording permission trong System Settings

### ✅ Step 2: Audio Capture
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "(First audio|System capture read)"
```
- Phải thấy: `✅ First audio buffer received!` và `✅ System capture read X samples`
- Nếu không: SCK helper không nhận được audio → Check capture strategy

### ✅ Step 3: Resampling
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "Resampled"
```
- Phải thấy: `📥 [Auto-transcription] Resampled X samples (48kHz) -> Y samples (16kHz)`
- Nếu không: Resampler chưa được gọi → Check code

### ✅ Step 4: Transcription
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "(Starting transcription|Raw transcription)"
```
- Phải thấy: `🔄 [Auto-transcription] Starting transcription` và `📝 [Auto-transcription] Raw transcription received (len=X)`
- Nếu `len=0`: Check sample rate hoặc audio content

### ✅ Step 5: Live Caption
```bash
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "LiveCaption.*Event"
```
- Phải thấy: `✅ [LiveCaption] Event emitted successfully`
- Nếu không: Check event emission code hoặc frontend listener

---

## 📝 Key Files Modified

1. **`src-tauri/src/managers/audio.rs`**
   - Thêm resampler initialization
   - Thêm resampling logic trong transcription loop
   - Thêm detailed logs

2. **`src-tauri/src/audio_toolkit/macos_audio_capture.swift`**
   - Đổi capture strategy từ display → all applications
   - Thêm debug logs cho delegate callbacks

3. **`src-tauri/src/audio_toolkit/system_audio_macos.rs`**
   - Forward SCK helper logs to frontend
   - Emit log events for debugging

---

## 🎯 Best Practices cho Dev Sau Này

### 1. **Luôn Check Log Trước Khi Code**
- Xem log để hiểu vấn đề ở đâu
- Không đoán mò, dựa vào log

### 2. **Thêm Log Chi Tiết**
- Log ở mỗi bước quan trọng
- Include context (sample count, buffer size, etc.)
- Use emoji để dễ filter: `🎙️`, `📥`, `🔄`, `📝`, `🎯`, `📤`, `✅`

### 3. **Test với Real Audio**
- Luôn test với Chrome đang phát video
- System luôn có audio để test

### 4. **Check Log Real-Time**
- Dùng `tail -f` để xem log real-time
- Filter log để focus vào vấn đề
- Check cả backend log và frontend console

### 5. **Verify End-to-End**
- Check từ audio capture → transcription → live caption
- Mỗi bước phải có log confirm
- Nếu một bước không có log → đó là vấn đề

---

## ✅ Kết Quả Cuối Cùng

**Trạng thái:** ✅ **HOẠT ĐỘNG**

**Log xác nhận:**
```
[2025-11-24][13:59:58][handy_app_lib::managers::audio][INFO] 🎯 [Auto-transcription] Result (len=41): 'And then two um uh the meeting pescope um'
[2025-11-24][13:59:58][handy_app_lib::managers::audio][INFO] ✅ [LiveCaption] Event emitted successfully
```

**Pipeline hoạt động:**
1. ✅ SCK helper nhận audio từ Chrome
2. ✅ Rust đọc samples từ buffer
3. ✅ Resampler: 48kHz → 16kHz
4. ✅ Transcription tạo text
5. ✅ Live caption events được emit
6. ✅ Frontend nhận và hiển thị caption

---

## 📚 Tài Liệu Tham Khảo

- **ScreenCaptureKit Documentation:** https://developer.apple.com/documentation/screencapturekit
- **Whisper Sample Rate:** 16kHz (16000 samples/second)
- **macOS System Audio:** 48kHz (48000 samples/second)
- **Rubato Resampler:** https://docs.rs/rubato/

---

**Tác giả:** Auto-generated từ debug session  
**Ngày:** 2025-11-24  
**Version:** 1.0

