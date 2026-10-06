# mgc-sbom

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng, source và ranh giới review của module. -->

## Chức năng

Tạo SBOM từ inventory dependency theo format CycloneDX/SPDX được hỗ trợ. Crate định nghĩa option cùng model component/dependency và serialize inventory; không resolve package hay chứng minh artifact đáng tin.

## Sơ đồ source

- [`lib.rs`](../src/lib.rs)
- [`generator.rs`](../src/generator.rs)
- [`cyclonedx.rs`](../src/cyclonedx.rs)
- [`spdx.rs`](../src/spdx.rs)

## Bàn giao và review

Xác thực identifier, tham chiếu dependency, dữ liệu license/version và ràng buộc format. Tách xác minh provenance/signature khỏi serialize.

## Xác minh

Sau khi sửa code, chạy test crate từ repo root bằng `cargo test -p mgc-sbom`. Với thay đổi chỉ có tài liệu, kiểm tra link, ngôn ngữ và Git ignore thay vì chạy test workspace không liên quan.
