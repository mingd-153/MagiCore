# mgc-fetcher

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Cung cấp helper download và giải nén archive tầng thấp dùng chung. Lớp cao sở hữu identity registry, auth, trust và policy install; crate này sở hữu cơ chế truyền/giải nén.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`download.rs`](../src/download.rs)
- [`extract.rs`](../src/extract.rs)

## Bàn giao và review

Review cả xác minh digest và policy path ở caller. Tải thành công không tự xác thực artifact; giữ giải nén an toàn và xử lý lỗi.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-fetcher`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
