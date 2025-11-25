# 📘 Hướng Dẫn Cài Đặt BlackHole và Cấu Hình Multi-Output Device

## 🎯 Mục Đích

Để sử dụng tính năng **System Audio Capture** (capture audio từ Chrome, YouTube, Spotify, v.v.), bạn cần:
1. Cài đặt **BlackHole** (virtual audio driver)
2. Tạo **Multi-Output Device** (để vừa nghe audio vừa capture được)
3. Cấu hình **Sound Output** trong System Settings

---

## 📥 Bước 1: Cài Đặt BlackHole

### **Cách 1: Sử dụng Homebrew (Khuyến nghị)**

```bash
# Cài đặt Homebrew nếu chưa có
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Cài đặt BlackHole 2ch
brew install blackhole-2ch
```

### **Cách 2: Download từ GitHub**

1. Truy cập: https://github.com/ExistentialAudio/BlackHole/releases
2. Download file `.pkg` mới nhất (ví dụ: `BlackHole-2ch.pkg`)
3. Mở file `.pkg` và làm theo hướng dẫn cài đặt
4. **Quan trọng:** Sau khi cài đặt, **khởi động lại Mac** để driver được load

### **Kiểm Tra Cài Đặt Thành Công**

Sau khi cài đặt và restart Mac:
1. Mở **System Settings** → **Sound**
2. Kiểm tra trong **Output** và **Input**:
   - Bạn sẽ thấy **"BlackHole 2ch"** trong danh sách
3. Nếu thấy → ✅ Cài đặt thành công!

---

## 🎛️ Bước 2: Tạo Multi-Output Device

**Tại sao cần Multi-Output Device?**
- Nếu chọn "BlackHole 2ch" trực tiếp làm Output → Bạn sẽ **KHÔNG nghe được** audio từ speakers
- Multi-Output Device cho phép audio route đến **CẢ** speakers **VÀ** BlackHole cùng lúc
- Kết quả: Bạn vừa nghe được audio, vừa capture được cho app

### **Hướng Dẫn Chi Tiết:**

1. **Mở Audio MIDI Setup**
   - Mở **Finder** → **Applications** → **Utilities** → **Audio MIDI Setup**
   - Hoặc dùng Spotlight: `Cmd + Space` → gõ "Audio MIDI Setup"

2. **Tạo Multi-Output Device**
   - Ở bottom-left của cửa sổ, click nút **"+"** (dấu cộng)
   - Chọn **"Create Multi-Output Device"**

3. **Cấu Hình Multi-Output Device**
   - Trong panel bên phải, bạn sẽ thấy danh sách devices
   - **Check box** cho các devices sau:
     - ✅ **BlackHole 2ch** (để app capture audio)
     - ✅ **Built-in Output** hoặc **Mac mini Speakers** (hoặc speakers của bạn - để nghe audio)
   
4. **Đặt Master Device**
   - Click vào **"Master Device"** dropdown
   - Chọn **"Built-in Output"** hoặc speakers của bạn (KHÔNG chọn BlackHole)
   - **Tại sao?** Master Device điều khiển volume - nếu chọn BlackHole, volume control sẽ không hoạt động

5. **Đặt Tên (Tùy chọn)**
   - Click đúp vào "Multi-Output Device" để đổi tên
   - Ví dụ: "Speakers + BlackHole" hoặc "Multi-Output"

### **Hình Ảnh Minh Họa:**

```
Audio MIDI Setup
┌─────────────────────────────────────┐
│  Devices                            │
│  ┌───────────────────────────────┐ │
│  │ Multi-Output Device           │ │
│  │                               │ │
│  │ ☑ BlackHole 2ch              │ │
│  │ ☑ Built-in Output            │ │
│  │                               │ │
│  │ Master Device:                │ │
│  │ [Built-in Output ▼]          │ │
│  └───────────────────────────────┘ │
│                                     │
│  [+] [-]                            │
└─────────────────────────────────────┘
```

---

## 🔊 Bước 3: Cấu Hình Sound Output

1. **Mở System Settings**
   - Click vào **Apple menu** (🍎) → **System Settings**
   - Hoặc `Cmd + ,` trong Finder

2. **Vào Sound Settings**
   - Click **Sound** trong sidebar
   - Hoặc tìm "Sound" trong search bar

3. **Chọn Multi-Output Device**
   - Trong tab **Output**, bạn sẽ thấy danh sách output devices
   - Chọn **Multi-Output Device** mà bạn vừa tạo (ví dụ: "Speakers + BlackHole")
   - **KHÔNG** chọn "BlackHole 2ch" trực tiếp (sẽ mất audio từ speakers)

4. **Kiểm Tra**
   - Phát một video trên YouTube hoặc Chrome
   - Bạn sẽ nghe được audio từ speakers ✅
   - App sẽ tự động capture audio từ BlackHole ✅

---

## ✅ Bước 4: Kiểm Tra Hoạt Động

1. **Mở Handy App**
   - Đảm bảo **System Audio** được bật trong Settings
   - Đảm bảo **Live Caption** được bật

2. **Phát Audio**
   - Mở Chrome/YouTube/Spotify
   - Phát một video có audio

3. **Kiểm Tra Logs**
   - Mở Debug Mode: `Cmd + Shift + D`
   - Hoặc xem logs: `tail -f ~/Library/Logs/com.pais.handy/handy.log`
   - Tìm các dòng:
     - `✅ Using BlackHole for system audio capture`
     - `✅✅✅ AUDIO DETECTED!`
     - `🎯 [Auto-transcription] Result`

4. **Kiểm Tra Live Caption**
   - Bạn sẽ thấy live caption hiển thị trong app
   - Caption sẽ update mỗi 3 giây khi có audio

---

## 🔧 Troubleshooting

### **Vấn đề 1: Không thấy "BlackHole 2ch" trong Sound Settings**

**Nguyên nhân:**
- BlackHole chưa được cài đặt đúng cách
- Mac chưa được restart sau khi cài đặt

**Giải pháp:**
1. Restart Mac
2. Kiểm tra lại trong System Settings → Sound
3. Nếu vẫn không thấy, cài đặt lại BlackHole:
   ```bash
   brew uninstall blackhole-2ch
   brew install blackhole-2ch
   ```
4. Restart lại Mac

### **Vấn đề 2: Audio không được capture (RMS = 0.000000)**

**Nguyên nhân:**
- Sound Output chưa được set đúng
- Multi-Output Device chưa được cấu hình đúng

**Giải pháp:**
1. Kiểm tra System Settings → Sound → Output
2. Đảm bảo đã chọn **Multi-Output Device** (không phải "BlackHole 2ch" trực tiếp)
3. Kiểm tra Audio MIDI Setup:
   - Multi-Output Device phải có **CẢ** BlackHole 2ch **VÀ** Built-in Output được check
   - Master Device phải là Built-in Output (không phải BlackHole)

### **Vấn đề 3: Nghe được audio nhưng không có live caption**

**Nguyên nhân:**
- App chưa detect được audio từ BlackHole
- Audio routing chưa đúng

**Giải pháp:**
1. Kiểm tra logs: `tail -f ~/Library/Logs/com.pais.handy/handy.log`
2. Tìm dòng: `Default OUTPUT device:`
3. Đảm bảo output là **Multi-Output Device** (không phải speakers trực tiếp)
4. Restart app sau khi cấu hình

### **Vấn đề 4: Volume control không hoạt động**

**Nguyên nhân:**
- Master Device trong Multi-Output Device được set là BlackHole

**Giải pháp:**
1. Mở Audio MIDI Setup
2. Chọn Multi-Output Device
3. Đổi **Master Device** thành **Built-in Output** (hoặc speakers của bạn)
4. Volume control sẽ hoạt động lại

---

## 📋 Checklist Hoàn Thành

Trước khi sử dụng System Audio Capture, đảm bảo:

- [ ] BlackHole đã được cài đặt và restart Mac
- [ ] "BlackHole 2ch" xuất hiện trong System Settings → Sound
- [ ] Multi-Output Device đã được tạo trong Audio MIDI Setup
- [ ] Multi-Output Device có **CẢ** BlackHole 2ch **VÀ** Built-in Output được check
- [ ] Master Device là Built-in Output (không phải BlackHole)
- [ ] System Settings → Sound → Output đã chọn Multi-Output Device
- [ ] App đã detect BlackHole (xem logs: `✅ Using BlackHole for system audio capture`)
- [ ] Audio được detect (xem logs: `✅✅✅ AUDIO DETECTED!`)
- [ ] Live caption đang hoạt động

---

## 🎓 Lưu Ý Quan Trọng

1. **Không chọn "BlackHole 2ch" trực tiếp làm Output**
   - Bạn sẽ KHÔNG nghe được audio từ speakers
   - Luôn dùng Multi-Output Device

2. **Master Device phải là speakers, không phải BlackHole**
   - Để volume control hoạt động đúng

3. **Restart Mac sau khi cài BlackHole**
   - Driver cần được load vào system

4. **Kiểm tra logs nếu có vấn đề**
   - Logs sẽ cho biết chính xác vấn đề ở đâu

---

## 📞 Hỗ Trợ

Nếu gặp vấn đề sau khi làm theo hướng dẫn:

1. Kiểm tra logs: `tail -f ~/Library/Logs/com.pais.handy/handy.log`
2. Tìm các dòng có `❌`, `⚠️`, hoặc `ERROR`
3. Xem thêm: [DEBUG_LIVE_CAPTION.md](DEBUG_LIVE_CAPTION.md)
4. Tạo issue trên GitHub với logs và mô tả vấn đề

---

## 🎉 Hoàn Thành!

Sau khi hoàn thành các bước trên, bạn có thể:
- ✅ Nghe audio từ speakers bình thường
- ✅ App tự động capture audio từ Chrome/YouTube/Spotify
- ✅ Live caption hiển thị real-time
- ✅ Transcription được paste vào active app

**Chúc bạn sử dụng vui vẻ!** 🚀

