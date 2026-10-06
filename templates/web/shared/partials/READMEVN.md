# Ghi chú các partial layer web dùng lại

## Chức năng

Mô tả các partial scaffold dự kiến được ghép vào dự án web và monorepo. Trong checkout này, chưa thư mục con nào có hợp đồng publish `template.toml`.

## Nội dung

- `base/` — README dự án và metadata web chung.
- `frontend-rust-ready/` — ghi chú crate Rust engine tùy chọn cho frontend.
- `monorepo-frontend-rust-ready/` — ranh giới Rust tùy chọn tương tự cho frontend monorepo.
- `monorepo/` — ghi chú khu vực frontend, backend và packages.
- `monorepo-packages/` — ghi chú cấu hình workspace dùng chung.

## Trạng thái

README này mô tả vai trò thư mục và giới hạn có bằng chứng từ cây file hiện tại. Hãy kiểm tra lại source, manifest và test trước khi dựa vào thay đổi sau này.
