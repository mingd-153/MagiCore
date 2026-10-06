# Adapter Cloud (mgc-cloud-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Phân loại CDK, Pulumi, Terraform và Cloudflare; điều phối scaffold/audit cùng các luồng lifecycle đã có. CDK/Pulumi có thể dùng lane dependency web nhúng. Lifecycle package Terraform và deploy engine do MagiCore sở hữu không được claim.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`cloud_type.rs`](../src/cloud_type.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`deploy/mod.rs`](../src/deploy/mod.rs)

## Bàn giao và review

Trace nhánh provider và capability trước khi claim deploy/dependency. Gọi CLI provider hay sinh scaffold không phải deployment plan do MagiCore sở hữu.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-cloud-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
