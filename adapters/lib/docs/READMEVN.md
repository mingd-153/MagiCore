# Adapter thư viện (mgc-lib-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Định tuyến TypeScript, Rust, Python, Go, Java/Kotlin và .NET theo manifest. Nhận diện rộng hơn quyền sở hữu native: Python chỉ nhận một tập pyproject, Maven khác Gradle, .NET cần project file. Core khác cũng nhúng một số helper native.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`language.rs`](../src/language.rs)
- [`manifest.rs`](../src/manifest.rs)
- [`native/engine.rs`](../src/native/engine.rs)

## Bàn giao và review

Dùng `dependency_manifest_format` và gate theo project; nhận diện ngôn ngữ chưa đủ. Khi được core khác nhúng, kiểm tra override caller, core ownership và lockfile tag.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-lib-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
