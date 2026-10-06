# mgc-config

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Sở hữu model cấu hình project/user, chuỗi registry, credential/registry mapping npmrc, hook và attestation core project. Caller nên dùng parser/policy tập trung thay vì tự tạo format cục bộ.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`project.rs`](../src/project.rs)
- [`chain.rs`](../src/chain.rs)
- [`npmrc.rs`](../src/npmrc.rs)
- [`attestation.rs`](../src/attestation.rs)

## Bàn giao và review

Cấu hình ảnh hưởng bảo mật/định tuyến. Trace ưu tiên, quyền sở hữu path, xử lý secret và mặc định fail-closed qua caller trước khi sửa parser.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-config`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
