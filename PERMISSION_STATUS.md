# Báo Cáo Trạng Thái System Audio Capture

## 🔍 Kiểm Tra Permission

**Trạng thái hiện tại:** ❌ **PERMISSION DENIED**

Log cho thấy:
```
❌ PERMISSION DENIED: The user declined TCCs for application, window, display capture
```

## 📋 Các Bước Khắc Phục

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
# Sau đó chạy lại
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

## 🔧 Debug Commands

```bash
# Xem log real-time
tail -f ~/Library/Logs/com.pais.handy/handy.log

# Filter log quan trọng
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(🔍|✅|❌|PERMISSION|First audio buffer|System capture read|🎯|Auto-transcription)"

# Kiểm tra process
ps aux | grep -E "handy|macos-audio-capture" | grep -v grep
```


