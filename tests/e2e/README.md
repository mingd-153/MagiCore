# E2E Pipeline — crate `mgc-e2e`

Black-box end-to-end tests driving the **real binaries** (`mgc`, `mgc-registry`) via subprocess — no nested cargo, no network beyond 127.0.0.1.

## Chạy

```bash
cargo build -p mgc --bin mgc -p mgc-registry-server --bin mgc-registry
cargo test -p mgc-e2e
```

Binary lookup: env `MGC_E2E_BIN_DIR` → `target/debug` → `target/release`. Không thấy → fail-fast kèm lệnh build.

## Kịch bản

| File | Flow |
|---|---|
| `tests/publish_install.rs` | pack → serve → publish → add/install → node_modules materialized |
| `tests/import_install.rs` | như trên + harvest lock entry thật → tổng hợp legacy `package-lock.json` → `mgc import` (signed) → xoá trace → `mgc install` phải seed graph từ lock đã import |

## CI

- Job `test`: bước 1 chạy `--exclude mgc --exclude mgc-e2e`; bước 2 (đã seed registry) chạy `-p mgc -p mgc-e2e`.
