# Web Adapter (`mgc-web-adapter`)

Adapter quản lý package NPM/Node.js cho MagiCore.

## Lệnh

| Lệnh | Mô tả |
|---|---|
| `mgc create <framework> <project> [--ts] [--tailwindcss] [--monorepo] [--backend <fw>]` | Scaffold dự án web đơn core |
| `mgc add <packages...> [-D] [-E] [-O] [-P] [--no-save] [-g]` | Thêm package cho core được chọn |
| `mgc install [packages...]` | Cài package cho core được chọn |
| `mgc create-web <framework> <project> [--ts] [--tailwindcss] [--monorepo] [--backend <fw>]` | Scaffold web trong dự án multi-core |
| `mgc add-web <packages...> [-D] [-E] [-O] [-P] [--no-save] [-g]` | Thêm package web trong dự án multi-core |
| `mgc remove-web <packages...>` | Gỡ package web |
| `mgc list-web` | Liệt kê package web |
| `mgc update-web [packages]` | Cập nhật package web |
| `mgc install-web [packages]` | Cài package web |

## Chạy test

```bash
cargo test -p mgc-web-adapter
cargo test -p mgc
```

## Benchmark

```bash
./scripts/bench.sh cold
./scripts/bench.sh stress
./scripts/bench.sh install
./scripts/bench.sh matrix
./scripts/bench.sh matrix-heavy
./scripts/bench.sh matrix-baseline
./scripts/bench.sh matrix-diff
./scripts/bench.sh matrix-heavy-baseline
./scripts/bench.sh matrix-heavy-diff
```

Ma trận benchmark chuẩn hóa năm trường hợp install/materialize: cache cục bộ lạnh với tarball có sẵn; cài lại khi cache nóng; resolve qua mock registry cục bộ với cache rỗng; khởi tạo từ shared cache; và cài offline khi registry không truy cập được nhưng cache đã có tarball/package. Profile `heavy` thêm đồ thị phụ thuộc lớn, sâu, có nhiều phiên bản trùng tên và tải materialize gần hơn với monorepo frontend. Ma trận này đo trước hành vi install/materialize; tải package lạnh từ registry trực tuyến là một hướng benchmark riêng.

Fallback metadata dùng `MAGICORE_WEB_METADATA_TTL_SECS` (mặc định `300`), `MAGICORE_WEB_METADATA_STALE_RETRY_TTL_SECS` (`30`) và giới hạn tuổi metadata cũ `MAGICORE_WEB_METADATA_MAX_STALE_SECS` (`604800`).

Shared cache được prune theo chu kỳ best-effort qua `MAGICORE_WEB_CACHE_PRUNE_INTERVAL_SECS` (`21600`); tarball, package đã giải nén và metadata cũ hết hạn theo `MAGICORE_WEB_CACHE_MAX_AGE_SECS` (`2592000`). Metadata đang chờ retry vẫn bị chặn khi vượt giới hạn tuổi tối đa; thời gian chờ retry không bỏ qua quy tắc an toàn này.

## Tích hợp

Các lệnh đi qua `cli/src/commands/core/web.rs`, tạo `WebAdapter` rồi ủy quyền phần logic dùng chung (`add`, `remove`, `list`, `update`, `install`) cho `shared.rs`.

Ở chế độ single-core, các lệnh trần như `mgc create`, `mgc add`, `mgc remove`, `mgc update`, `mgc list`, `mgc install` dùng đường dẫn tự nhận diện core. Dự án multi-core có các lệnh riêng như `mgc create-web`, `mgc add-web`, `mgc install-web`.
