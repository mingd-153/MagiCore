# Adapter CI/CD (mgc-cicd-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Nhận diện GitHub Actions, GitLab, CircleCI, Cloudflare, AWS, GCP và Argo CD; cung cấp tích hợp scaffold/audit. File pipeline vẫn do project sở hữu; adapter không công bố resolve/fetch/install dependency registry hay quản lý lockfile.

## Sơ đồ source

- [`adapter.rs`](../src/adapter.rs)
- [`provider.rs`](../src/provider.rs)
- [`lib.rs`](../src/lib.rs)

## Bàn giao và review

Tách nhận diện/scaffold/audit khỏi chạy hoặc deploy pipeline. Kiểm tra `CicdAdapter::CAPABILITIES`; thao tác package chưa hỗ trợ không được thành công im lặng.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-cicd-adapter`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
