# Workspace scaffold web

## Chức năng

Gom các template dự kiến cho dự án web và các mảnh scaffold dùng chung. Tra cứu production dùng registry cache, trừ khi caller đặt tường minh `MAGICORE_TEMPLATE_DIR`; đường dẫn trong repo không phải nguồn mặc định. Checkout này không có layer `template.toml` hay payload source/config dự án; chỉ có các README mô tả.

## Nội dung

- `shared/partials/` — ghi chú placeholder cho các mảnh scaffold base, Rust-ready và monorepo dùng lại.
- Generator dưới `scripts/` mong đợi các đường dẫn template web ở đây khi sinh file template.

## Trạng thái

README này mô tả vai trò thư mục và giới hạn có bằng chứng từ cây file hiện tại. Hãy kiểm tra lại source, manifest và test trước khi dựa vào thay đổi sau này.
