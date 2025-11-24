# Hướng Dẫn: Cấp Screen Recording Permission

## 📸 Từ Ảnh Bạn Gửi

Tôi thấy trong System Settings > Privacy & Security > Screen Recording:
- ✅ **Terminal** đã được BẬT (ON)

## 🔍 Tình Huống

### Nếu đang chạy `bun tauri dev`:
- ✅ **Terminal đã có permission rồi!**
- Chỉ cần **restart app** là được
- App "Handy" sẽ KHÔNG xuất hiện trong danh sách vì đang chạy qua Terminal

### Nếu muốn thêm "Handy" vào danh sách:
- Cần **build và chạy app** (không phải `bun tauri dev`)
- Khi app build chạy lần đầu và request permission, macOS sẽ hiển thị dialog
- Sau khi click "Allow", "Handy" sẽ xuất hiện trong danh sách

## 📋 Các Bước

### Option 1: Dùng Terminal Permission (Đơn Giản Nhất)

1. ✅ Terminal đã ON trong Screen Recording (từ ảnh)
2. Restart app:
   ```bash
   pkill -9 -f "handy|macos-audio-capture"
   bun tauri dev
   ```
3. Kiểm tra log:
   ```bash
   tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(🔍|✅|PERMISSION|First audio buffer)"
   ```

**Log mong đợi:**
- `✅ PERMISSION GRANTED - Found X displays`
- `✅ First audio buffer received!`

### Option 2: Build App và Dùng "Handy" Permission

1. Build app:
   ```bash
   bun run tauri build
   ```

2. Chạy app build:
   ```bash
   open src-tauri/target/release/bundle/macos/Handy.app
   # Hoặc
   open src-tauri/target/debug/bundle/macos/Handy.app
   ```

3. Khi app chạy, nó sẽ tự động request Screen Recording permission
4. Click "Allow" trong dialog
5. "Handy" sẽ xuất hiện trong System Settings > Screen Recording
6. Đảm bảo toggle cho "Handy" là ON

## ⚠️ Lưu Ý Quan Trọng

1. **macOS chỉ hiển thị app trong danh sách sau khi app đã request permission**
   - Nếu chưa request → không thấy trong danh sách
   - Cần trigger permission dialog bằng cách chạy app

2. **Permission được gắn với process cụ thể:**
   - `bun tauri dev` → Terminal
   - App build → Handy

3. **Sau khi cấp permission, bắt buộc phải restart app**

## 🔧 Debug

```bash
# Kiểm tra permission status trong log
tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -E "(🔍|✅|❌|PERMISSION)"

# Kiểm tra process đang chạy
ps aux | grep -E "handy|macos-audio-capture" | grep -v grep

# Kiểm tra Terminal permission
tccutil reset ScreenCapture com.apple.Terminal
# Sau đó chạy lại app để trigger dialog
```

## ✅ Kết Luận

**Với Terminal đã ON, bạn chỉ cần restart app là được!**

App "Handy" sẽ không xuất hiện trong danh sách nếu đang chạy `bun tauri dev` vì permission được gắn với Terminal process.

