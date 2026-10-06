# Adapter Web (mgc-web-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Triển khai vòng đời npm/Node.js: nhận diện, resolve, registry, kiểm tra lock, fetch/materialize an toàn, lifecycle policy, cache, audit, list/update và SBOM. Đây là adapter package manager hoàn chỉnh nhất checkout; dispatch CLI và scaffold ở lớp khác.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`install/mod.rs`](../src/install/mod.rs)
- [`provider.rs`](../src/provider.rs)
- [`supply_chain.rs`](../src/supply_chain.rs)

## Bàn giao và review

Trace CLI, quyền sở hữu manifest/lock, capability và lane install. Giữ policy integrity, URL registry, lifecycle script và stale cache; không khái quát bảo đảm npm sang core khác.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-web-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
