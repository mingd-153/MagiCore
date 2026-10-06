# IoT Adapter (mgc-iot-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Detects ESP32 Rust, PlatformIO, and Zephyr and provides board/target metadata, scaffolding, install helpers, and explicit flash integration. General dependency resolution is not an adapter capability today; device flashing stays at the selected hardware/toolchain boundary.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`framework.rs`](../src/framework.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`flash/mod.rs`](../src/flash/mod.rs)

## Handoff and review

Separate target metadata from hardware execution. Trace whether a flow uses a MagiCore-owned engine or delegates to an external toolchain; report unsupported/device-missing cases explicitly.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-iot-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
