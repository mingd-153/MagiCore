# mgc-registry-server

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Triển khai private server `mgc-registry` cùng luồng npm/PyPI/OCI, auth, storage, rate limit và trusted publishing/provenance. Runtime config điều khiển listener, giới hạn, backend, OIDC issuer/audience và trusted proxy; trace config/middleware tới handler trước khi claim bảo đảm.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`auth.rs`](../src/auth.rs)
- [`npm.rs`](../src/npm.rs)
- [`storage.rs`](../src/storage.rs)
- [`trusted.rs`](../src/trusted.rs)

## Bàn giao và review

Review issuer/audience/subject, yêu cầu admin token, proxy trust, scope/replay package, signing key và giả định storage. Đăng ký route chưa chứng minh phân quyền đúng.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-registry-server`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
