<p align="center"><img src="assets/logo.svg" alt="MegaGate logo" width="900"/></p>

# MegaGate

**MegaGate** is a Rust-first multi-core package and project manager.

Hiện tại repo đang bootstrap chắc trước cho:

- `megagate` -> full multi-core build
- `megagate-web` -> single-core web build

Web là core được triển khai sâu nhất ở vòng này, và đang là chuẩn để mở rộng runtime dần sang các core còn lại.

## Current shape

- unified CLI binary: `mg`
- single-core và multi-core command surface tách riêng
- web adapter có resolver / lockfile / install path / scaffold riêng
- packaging manifest đã có cho toàn bộ core package

## Build local distributions

```bash
# Full multi-core build
./scripts/build.sh megagate

# Single-core web build
./scripts/build.sh megagate-web

# Build both bootstrap packages
./scripts/build.sh bootstrap

# Build every packaged core shape
./scripts/build.sh all
```

Artifacts được đặt ở:

```bash
dist/megagate/<target>/mg
dist/megagate-web/<target>/mg
```

## Build profile behavior

### `megagate`

- build mode: multi-core
- expected command shape:
  - `mg create-web ...`
  - `mg add-web ...`
  - `mg install-web ...`
  - các core khác dùng `create-<core>`, `add-<core>`, `install-<core>`

### `megagate-web`

- build mode: single-core web
- expected command shape:
  - `mg create ...`
  - `mg add ...`
  - `mg install ...`

## Key references

- [scripts/DESIGN_FLOW.md](/Users/doanmihh/Documents/Workspace/MegaGate/scripts/DESIGN_FLOW.md:1)
- [scripts/CLI_INSTALL_MODEL.md](/Users/doanmihh/Documents/Workspace/MegaGate/scripts/CLI_INSTALL_MODEL.md:1)
- [scripts/MEGAGATE_BOOTSTRAP_RELEASE.md](/Users/doanmihh/Documents/Workspace/MegaGate/scripts/MEGAGATE_BOOTSTRAP_RELEASE.md:1)
- [REPORT.md](/Users/doanmihh/Documents/Workspace/MegaGate/REPORT.md:1)

## Packaged core matrix

Repo hiện đã có manifest cho:

- `megagate`
- `megagate-web`
- `megagate-ai`
- `megagate-game`
- `megagate-clo`
- `megagate-cicd`
- `megagate-iot`
- `megagate-app`
- `megagate-lib`

Phần runtime thật vẫn đang đi từ web ra các core còn lại.
