# mgc-types

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Định nghĩa contract/kiểu dùng chung: adapter/capability, ecosystem, identity package/version, manifest, model publish/patch, lỗi/result, JSONC và che secret. Thay đổi public có thể ảnh hưởng đa số crate; cần review caller và tương thích serialization/API.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`adapter.rs`](../src/adapter.rs)
- [`capabilities.rs`](../src/capabilities.rs)
- [`package.rs`](../src/package.rs)
- [`manifest.rs`](../src/manifest.rs)

## Bàn giao và review

Phân tích caller trước khi sửa trait/enum public hoặc identity. Kiểm tra serialization, equality/order/hash, chuỗi ecosystem ownership và core parity.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-types`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
