# Các Cách Khác Để Capture System Audio trên macOS

## 📋 Tổng Quan

macOS ScreenCaptureKit không gửi audio buffers trong nhiều trường hợp, mặc dù đã cấu hình đúng. Đây là hạn chế của macOS ScreenCaptureKit.

**✅ ĐÃ IMPLEMENT:** BlackHole support đã được thêm vào app. App sẽ tự động thử BlackHole trước, sau đó fallback về ScreenCaptureKit nếu BlackHole không có sẵn.

## 🔧 Các Giải Pháp Thay Thế

### 1. **BlackHole** (Khuyến nghị) ⭐

**Mô tả:** Virtual audio driver cho phép route system audio đến một virtual device, sau đó capture từ device đó.

**Ưu điểm:**
- Hoạt động ổn định và đáng tin cậy
- Không cần permission đặc biệt (chỉ cần microphone permission)
- Hỗ trợ nhiều channels
- Open source và miễn phí

**Nhược điểm:**
- User cần cài đặt BlackHole driver
- User cần cấu hình Sound preferences để route audio qua BlackHole

**Cách sử dụng:**
1. User cài đặt BlackHole: `brew install blackhole-2ch` hoặc download từ [GitHub](https://github.com/ExistentialAudio/BlackHole)
2. **QUAN TRỌNG:** User cấu hình Sound preferences:
   - System Settings > Sound > Output: Chọn "BlackHole 2ch"
   - Nếu không cấu hình, app sẽ capture silence (RMS = 0)
3. App tự động detect và sử dụng BlackHole nếu có sẵn
   - System Settings > Sound > Input: Chọn "BlackHole 2ch"
3. App capture từ BlackHole device như một microphone thông thường

**Implementation:**
- Sử dụng `cpal` hoặc `rodio` để enumerate audio devices
- Tìm device có name chứa "BlackHole"
- Capture từ device đó như microphone input

**Code example:**
```rust
// Trong audio_toolkit/system_audio_macos.rs
// Thay vì dùng ScreenCaptureKit, enumerate devices và tìm BlackHole
let devices = cpal::devices()?;
for device in devices {
    if device.name()?.contains("BlackHole") {
        // Capture từ device này
    }
}
```

### 2. **Soundflower** (Tương tự BlackHole)

**Mô tả:** Tương tự BlackHole, nhưng ít được maintain hơn.

**Ưu điểm:**
- Tương tự BlackHole

**Nhược điểm:**
- Ít được maintain hơn BlackHole
- Có thể không hoạt động trên macOS mới nhất

### 3. **CoreAudio với Aggregate Device**

**Mô tả:** Tạo một aggregate device kết hợp system audio và microphone.

**Ưu điểm:**
- Không cần user cài đặt driver bên ngoài
- Hoạt động với CoreAudio native

**Nhược điểm:**
- Phức tạp hơn để implement
- Có thể không capture được system audio trực tiếp (vẫn cần virtual device)

### 4. **AVAudioEngine với Input Node**

**Mô tả:** Sử dụng AVAudioEngine để capture từ input device.

**Ưu điểm:**
- Native macOS API
- Không cần external driver

**Nhược điểm:**
- Không capture được system audio trực tiếp (chỉ capture từ microphone)
- Vẫn cần virtual device như BlackHole

## 🎯 Khuyến Nghị

**Giải pháp tốt nhất:** Sử dụng **BlackHole** vì:
1. Hoạt động ổn định và đáng tin cậy
2. Được maintain tốt
3. Dễ implement (chỉ cần capture từ device như microphone)
4. User experience tốt (chỉ cần cài đặt một lần và cấu hình)

**Implementation Plan:**
1. Detect xem BlackHole có được cài đặt không
2. Nếu có, capture từ BlackHole device
3. Nếu không, fallback về ScreenCaptureKit (hoặc hiển thị hướng dẫn cài đặt)
4. Hiển thị hướng dẫn cài đặt BlackHole trong UI nếu cần

## 📝 Next Steps

1. Implement BlackHole detection
2. Implement capture từ BlackHole device
3. Thêm UI để hướng dẫn user cài đặt BlackHole
4. Test với nhiều trường hợp khác nhau
5. Document trong README và user guide

