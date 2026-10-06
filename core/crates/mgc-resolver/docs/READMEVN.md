# mgc-resolver

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Resolve dependency graph với version set, cache registry, patch và PubGrub. Protocol gồm crates, Go, Maven, NuGet, Pub, PyPI, Swift, React Native, Hugging Face; npm resolve ở web adapter. HTTP registry dùng `mgc-http` để áp dụng policy mạng có giới hạn.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`graph.rs`](../src/graph.rs)
- [`protocols/mod.rs`](../src/protocols/mod.rs)
- [`solver/pubgrub.rs`](../src/solver/pubgrub.rs)
- [`patches.rs`](../src/patches.rs)

## Bàn giao và review

Kiểm tra thứ tự version, metadata transitive, truyền lỗi, cache freshness và ánh xạ lockfile tag. Giữ boundary HTTP chung và không bỏ qua lỗi im lặng.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-resolver`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
