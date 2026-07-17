# Báo Cáo Toàn Bộ Bề Mặt Lệnh (CLI Surface) Của MegaGate (V3 - Hiện Trạng Thực Tế)

Ngày lập: 2026-07-16

---

## I. Cờ Toàn Cục (Global Flags)
- `-h, --help`: Hiển thị trợ giúp chi tiết.
- `-V, --version`: Phiên bản CLI.
- `--core <CORE_NAME>`: Ép buộc lệnh chạy dưới bối cảnh của một Core cụ thể.
- `--audit-strict`: Hủy bỏ tiến trình cài đặt nếu phát hiện package mới xuất bản dưới 24h hoặc có lỗ hổng CVE.
- `-r, --recursive`: Chạy lệnh đệ quy cho toàn bộ Workspace/Monorepo. *(đã định nghĩa, chưa implement)*

---

## II. Nhóm Lệnh Tiện Ích Chung (Common Commands)

### 1. `mg init`
- Wizard hỏi ecosystem, framework, toolchain.
- Tạo `mg.toml` ở project root (không phải `.megagate/project.toml`).
- Cờ: `--template <Name>` (Bỏ qua Wizard).

### 2. `mg info <pkg>`
- Truy xuất NPM registry. Gán nhãn Core bằng heuristic.
- Cờ: `--json`.

### 3. `mg search <query>`
- Quét NPM registry, gán nhãn Core bằng heuristic.
- Cờ: `--exact`, `--page <NUM>`, `--json`, `--core <CORE>`.

### 4. `mg audit`
- Web: mock. Các core khác: no-op.

### 5. `mg outdated`
- Quét gói lạc hậu từ NPM registry.
- Cờ: `--json`.

---

## III. Nhóm Lệnh Native Engine

### 1. `mg dev`
- Axum server + `notify` file watcher.
- WebSocket HMR (`/hmr` endpoint — "connected" message confirmed).
- `--host <IP>`, `--port <NUM>`, `--clear` (clear terminal trước khi start).
- Default port 4315.

### 2. `mg build`
- **Rust project** (Cargo.toml): chạy `cargo build`.
- **Web project** (package.json): native esbuild bundler (esbuild-rs crate, không shell).
- Các core khác: chưa implement.

### 3. `mg start`
- Web Server siêu nhẹ cho Production (Axum/hyper). Phục vụ tĩnh từ `dist/`, `build/`, `.next/`, `out/`, `public/`.

### 4. `mg run <script>`
- Task Runner. Thực thi script trong `mg.toml` hoặc `package.json` qua `sh -c`.

### 5. `mg exec <cmd>`
- Thực thi shell command trong bối cảnh project. PATH bao gồm `node_modules/.bin` cho web.

### 6. `mg dlx <pkg>`
- Tải package vào `~/.cache/megagate/dlx/` và chạy mà không cần cài đặt.

---

## IV. Nhóm Lệnh Quản Lý Dependency

1. **`mg install`** (alias: `mg i`)
   - `--frozen`: Ép buộc cài đúng version theo lockfile.
   - `--ignore-scripts`: Chặn post-install scripts.
2. **`mg add <packages...>`** (alias: `mg a`)
   - `-D, --dev`, `-g, --global`, `-E, --exact`, `-O, --optional`, `-P, --peer`
   - `--no-save`, `-v, --version <VER>`
3. **`mg remove <package>`** (alias: `mg rm`)
4. **`mg update <packages...>`** (alias: `mg up`)
   - `--install`: Tự động tải và cài đặt bản cập nhật.
5. **`mg list`** (alias: `mg ls`)
6. **`mg link [package]`** — native symlink (không shell)
7. **`mg unlink [package]`** — native remove symlink
8. **`mg why <package>`** — native parse mg.lock, show reverse deps

---

## IV. Nhóm Lệnh Namespaced (8 Cores)

### 1. Khởi tạo
- `mg create-web <framework> <project_name>` (Kèm cờ: `--ts`, `--tailwind`, `--eslint`, `--src-dir`, `--app-router`)
- `mg create-game/ai/clo/cicd/iot/app/lib <framework> <project_name>` *(stub)*

### 2. Quản lý rẽ nhánh
Mỗi Core có bộ 5 lệnh:
- `mg install-<core>` *(chỉ web có implement thật)*
- `mg add-<core> <packages...>` *(chỉ web có implement thật)*
- `mg remove-<core> <package>` *(chỉ web có implement thật)*
- `mg update-<core> <packages...>` *(chỉ web có implement thật)*
- `mg list-<core>` *(chỉ web có implement thật)*

---

**Tổng kết:**
- `mg build` build được Rust project (cargo build) + Web project (esbuild native).
- `mg link/unlink/why` native, không shell.
- `mg dev` có WebSocket HMR + `--clear`.
- `mg.toml` ở project root (không `.megagate/`).
- Aliases: `mg i/rm/up/ls`.
- 7 core còn lại (game, ai, clo, cicd, iot, app, lib): stub.