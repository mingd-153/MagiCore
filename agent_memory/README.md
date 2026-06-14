# agent_memory

Thư viện **agent_memory** cung cấp một kho nhớ toàn cục (in‑memory) cho các AI‑Agent trong dự án Rust. Nó cho phép Agent **gắn thẻ (tag)**, lưu trữ và truy xuất dữ liệu trạng thái một cách nhanh chóng mà không cần cơ sở dữ liệu bên ngoài.

## 📦 Thêm dependency
```toml
[dependencies]
agent_memory = { path = "./agent_memory" }
```
(đã được thêm trong `hyper-pkg/Cargo.toml`)

## 🔧 API chính
```rust
use agent_memory::trellis::Trellis;
```
| Hàm | Mô tả | Trả về |
|-----|-------|--------|
| `Trellis::put(key, value)` | Lưu một giá trị dưới **khóa phân cấp** (`key` dạng `a.b.c`). | `anyhow::Result<()>` |
| `Trellis::fetch(key)` | Đọc giá trị đã lưu. | `anyhow::Result<Option<String>>` |
| `Trellis::delete(key)` | Xóa khóa. | `anyhow::Result<Option<String>>` |

## 🧠 Cách dùng trong AI‑Agent
### 1. Gắn thẻ (tag) cho một hành động
```rust
// Khi agent thực hiện một hành động, lưu trạng thái để các bước sau có thể dùng lại
Trellis::put("agent.last_action", "search_news")?;   // tag hành động cuối
Trellis::put("agent.search.query", "AI safety")?;   // tag dữ liệu đầu vào
```
### 2. Lấy lại thông tin đã gắn thẻ
```rust
if let Some(action) = Trellis::fetch("agent.last_action")? {
    println!("Hành động cuối cùng: {}", action);
}
```
### 3. Xóa tag khi không còn cần
```rust
Trellis::delete("agent.last_action")?;
```

## 🛠️ Kịch bản mẫu cho một AI‑Agent
```rust
async fn handle_message(msg: &str) -> anyhow::Result<()> {
    // Kiểm tra xem có query đã lưu trước đó không
    if let Some(prev) = Trellis::fetch("agent.prev_query")? {
        println!("Tiếp tục với query trước: {}", prev);
        return Ok(());
    }
    // Nếu chưa có, lưu query hiện tại làm tag
    Trellis::put("agent.prev_query", msg)?;
    // … thực thi logic AI …
    Ok(())
}
```

## 📚 Lưu ý
* Bộ nhớ chỉ tồn tại trong thời gian chạy của chương trình (không bền vững). Đối với lưu trữ lâu dài, cần tích hợp cơ chế ghi file hoặc DB.
* Nên dùng **khóa dạng phân cấp** (`namespace.key`) để tránh xung đột khi nhiều agent chia sẻ bộ nhớ.

---

*Thư viện này nhẹ, không phụ thuộc vào async runtime; bạn có thể dùng đồng thời trong các task async hoặc sync.*
