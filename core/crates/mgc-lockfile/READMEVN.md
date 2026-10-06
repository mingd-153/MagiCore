# `mgc-lockfile` — Engine lockfile thống nhất

Crate này cung cấp định dạng `mgc.lock` thống nhất, metadata kiểm tra toàn vẹn và khả năng import lockfile từ các package manager khác (npm, pnpm, yarn và bun).

## Khả năng

1. **Serialize và deserialize:** đọc, ghi biểu diễn TOML và JSON của `mgc.lock`.
2. **Kiểm tra checksum:** tạo và xác minh `mgc.lock.sha256`.
3. **Chữ ký BLAKE3 có khóa:** xác thực lockfile khi cấu hình `MAGICORE_LOCKFILE_KEY`.
4. **Import giữa các package manager:** hỗ trợ `package-lock.json` (npm v2/v3), `pnpm-lock.yaml` (pnpm v6/v9), `yarn.lock` (Yarn Classic v1) và `bun.lock` (Bun v1 JSON).

## Cách dùng

### Đọc và xác minh lockfile

```rust
use std::path::Path;
use mgc_lockfile::read_lockfile_checked;

let project_root = Path::new("./my-project");
if let Some(lockfile) = read_lockfile_checked(project_root)? {
    println!("Core: {}, Packages: {}", lockfile.core, lockfile.packages.len());
}
```

### Ghi lockfile kèm checksum

```rust
use std::path::Path;
use mgc_lockfile::{Lockfile, write_lockfile};

let lockfile = Lockfile::new("web", "frontend");
write_lockfile(Path::new("./my-project"), &lockfile)?;
```

### Import lockfile cũ

```rust
use std::path::Path;
use mgc_lockfile::import::import_legacy_lockfile_explicit;
use mgc_types::Manifest;

let project_root = Path::new("./legacy-project");
let manifest = Manifest::new("app", mgc_types::Ecosystem::Web);
if let Some(migrated_lock) = import_legacy_lockfile_explicit(
    project_root, "web", "frontend", &manifest
)? {
    println!("Đã chuyển đổi thành công {} package!", migrated_lock.packages.len());
}
```

## Chạy test

Chạy unit test và integration test của crate bằng lệnh:

```bash
cargo test -p mgc-lockfile
```

Xem [`README.md`](README.md) để đọc hướng dẫn tiếng Anh.
