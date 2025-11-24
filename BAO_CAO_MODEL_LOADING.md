# Báo Cáo: Lỗi "Failed to load available models"

## 🔍 Vấn Đề

App hiển thị lỗi "Failed to load available models" trong Onboarding screen.

## 🔧 Đã Sửa

### 1. Cải Thiện Error Handling
- Thêm retry logic với 3 lần thử
- Delay 500ms giữa các lần thử để đảm bảo backend đã sẵn sàng
- Hiển thị error message chi tiết hơn

**File:** `src/components/onboarding/Onboarding.tsx`

### 2. Kiểm Tra Backend
- ModelManager đã được khởi tạo đúng trong `initialize_core_logic()`
- Command `get_available_models` đã được register trong Tauri
- ModelManager đã được thêm vào app state

## 📋 Nguyên Nhân Có Thể

1. **Timing Issue:**
   - Onboarding component mount trước khi ModelManager được khởi tạo
   - Backend chưa sẵn sàng khi frontend gọi command

2. **ModelManager Initialization Error:**
   - Có thể có lỗi khi khởi tạo ModelManager
   - Cần kiểm tra log để xem có lỗi gì không

## ✅ Giải Pháp Đã Áp Dụng

1. **Retry Logic:**
   - Thử lại 3 lần với delay 500ms
   - Đảm bảo backend có thời gian khởi tạo

2. **Better Error Messages:**
   - Hiển thị error message chi tiết từ backend
   - Giúp debug dễ dàng hơn

## 🔍 Debug Steps

Nếu vẫn gặp lỗi, kiểm tra:

1. **Console Log:**
   ```bash
   # Mở DevTools trong browser và xem Console tab
   # Tìm error message chi tiết
   ```

2. **Backend Log:**
   ```bash
   tail -f ~/Library/Logs/com.pais.handy/handy.log | grep -iE "(model|error|failed)"
   ```

3. **ModelManager Status:**
   - Kiểm tra xem ModelManager có được khởi tạo thành công không
   - Kiểm tra xem models directory có được tạo không

## 📝 Next Steps

1. Test lại app sau khi restart
2. Kiểm tra console log để xem error message chi tiết
3. Nếu vẫn lỗi, kiểm tra ModelManager initialization

