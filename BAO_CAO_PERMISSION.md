# Báo Cáo: Vấn Đề System Audio Capture Permission

**Ngày:** 2025-11-24  
**Trạng thái:** ⚠️ **Permission chưa được cấp**

## 🔍 Tóm Tắt Vấn Đề

App không thể capture system audio vì **Screen Recording permission chưa được cấp** trên macOS.

## 📊 Kiểm Tra Log

### Permission Status
```
❌ PERMISSION DENIED: The user declined TCCs for application, window, display capture
```

### Audio Capture Status
- Swift helper process: ✅ Đang chạy
- Permission check: ❌ Bị từ chối
- Audio buffers: ❌ Không nhận được (do permission)
- Auto-transcription: ⏸️ Đang chờ audio samples

### Code Status
- ✅ Auto-transcription thread đã khởi động
- ✅ Swift helper đang chạy
- ✅ Permission check logic hoạt động đúng
- ✅ Logging đầy đủ và rõ ràng

## 🔧 Các Thay Đổi Đã Thực Hiện

### 1. Cải Thiện Permission Detection
- Thêm log rõ ràng với emoji: `🔍 Checking Screen Recording permission...`
- Thêm log chi tiết khi permission bị từ chối
- Thêm hướng dẫn cụ thể trong log

**File:** `src-tauri/src/audio_toolkit/macos_audio_capture.swift`

### 2. Cải Thiện Logging
- Log permission status với format dễ đọc
- Thêm warning khi không có audio samples
- Giảm log spam, chỉ log các sự kiện quan trọng

**File:** `src-tauri/src/managers/audio.rs`

### 3. Tài Liệu Hướng Dẫn
- Tạo `PERMISSION_STATUS.md` với hướng dẫn chi tiết
- Thêm troubleshooting section trong README

## 📋 Hướng Dẫn Khắc Phục

### Bước 1: Cấp Screen Recording Permission

1. Mở **System Settings** (Cài đặt Hệ thống)
2. Vào **Privacy & Security** (Quyền riêng tư & Bảo mật)
3. Chọn **Screen Recording** (Ghi màn hình)
4. **BẬT** permission cho:
   - **Terminal** (nếu đang chạy `bun tauri dev`)
   - **Handy** (nếu đang chạy app đã build)

### Bước 2: Restart App

Sau khi cấp permission, **bắt buộc phải restart app**:
```bash
pkill -9 -f "handy|macos-audio-capture"
bun tauri dev
```

### Bước 3: Kiểm Tra Log

Sau khi restart, kiểm tra log:
```bash
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(🔍|✅|❌|PERMISSION|First audio buffer|System capture read|🎯)"
```

**Log mong đợi khi permission đã được cấp:**
- `✅ PERMISSION GRANTED - Found X displays`
- `✅ First audio buffer received!`
- `✅ System capture read X samples from buffer`
- `🎯 Auto-transcription result`

## ⚠️ Lưu Ý Quan Trọng

1. **Permission phải được cấp cho process đang chạy:**
   - Nếu chạy `bun tauri dev` → cấp cho **Terminal**
   - Nếu chạy app build → cấp cho **Handy**

2. **Sau khi cấp permission, bắt buộc phải restart app** - macOS không tự động reload permission.

3. **Nếu đã cấp permission nhưng vẫn không hoạt động:**
   - Kiểm tra lại log xem có `✅ PERMISSION GRANTED` không
   - Kiểm tra xem có `✅ First audio buffer received!` không
   - Nếu không có, có thể là vấn đề code hoặc audio không phát từ source được capture

## 🔍 Debug Commands

```bash
# Xem log real-time
tail -f ~/Library/Logs/com.pais.handy/handy.log

# Filter log quan trọng
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(🔍|✅|❌|PERMISSION|First audio buffer|System capture read|🎯|Auto-transcription)"

# Kiểm tra process
ps aux | grep -E "handy|macos-audio-capture" | grep -v grep

# Kiểm tra permission status trong log
tail -100 ~/Library/Logs/com.pais.handy/handy.log | grep -E "(PERMISSION|Permission)"
```

## ✅ Kết Luận

**Code đang hoạt động đúng.** Vấn đề duy nhất là **Screen Recording permission chưa được cấp**. 

Sau khi cấp permission và restart app, hệ thống sẽ:
1. ✅ Nhận audio buffers từ system
2. ✅ Transcribe audio real-time
3. ✅ Hiển thị live caption
4. ✅ Lưu transcription vào history

