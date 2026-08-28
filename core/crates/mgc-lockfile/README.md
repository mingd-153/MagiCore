# `mgc-lockfile` — Unified Lockfile Engine

Crate cung cấp cơ chế quản lý lockfile thống nhất (`mgc.lock`), chữ ký số BLAKE3, và công cụ chuyển đổi (import) các lockfile từ hệ sinh thái khác (npm, pnpm, yarn, bun).

---

## 🚀 Tính năng

1. **Serialize / Deserialize**: Chuyển đổi hai chiều định dạng TOML & JSON cho `mgc.lock`.
2. **Checksum Integrity**: Tự động sinh và kiểm tra `mgc.lock.sha256`.
3. **Chữ ký số BLAKE3 (Keyed Hash)**: Bảo vệ chống giả mạo lockfile khi có biến môi trường `MAGICORE_LOCKFILE_KEY`.
4. **Cross-PM Migration Engine**:
   - `package-lock.json` (npm v2, v3)
   - `pnpm-lock.yaml` (pnpm v6, v9)
   - `yarn.lock` (yarn classic v1)
   - `bun.lock` (bun v1 JSON)

---

## 📖 Hướng Dẫn Sử Dụng

### 1. Đọc và Kiểm Tra Lockfile
```rust
use std::path::Path;
use mgc_lockfile::read_lockfile_checked;

let project_root = Path::new("./my-project");
if let Some(lockfile) = read_lockfile_checked(project_root)? {
    println!("Core: {}, Packages: {}", lockfile.core, lockfile.packages.len());
}
```

### 2. Ghi Lockfile Kèm Checksum
```rust
use std::path::Path;
use mgc_lockfile::{Lockfile, write_lockfile};

let mut lockfile = Lockfile::new("web", "frontend");
write_lockfile(Path::new("./my-project"), &lockfile)?;
```

### 3. Import Lockfile Từ npm/pnpm/yarn/bun
```rust
use std::path::Path;
use mgc_lockfile::import::import_legacy_lockfile_explicit;
use mgc_types::Manifest;

let project_root = Path::new("./legacy-project");
let manifest = Manifest::new("app", mgc_types::Ecosystem::Web);
if let Some(migrated_lock) = import_legacy_lockfile_explicit(project_root, "web", "frontend", &manifest)? {
    println!("Migrated {} packages successfully!", migrated_lock.packages.len());
}
```

---

## 🧪 Hướng Dẫn Chạy Test

Chạy toàn bộ unit tests và integration tests của crate:
```bash
cargo test -p mgc-lockfile
```
