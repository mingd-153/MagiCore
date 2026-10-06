# Game Adapter (mgc-game-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Detects Bevy, Godot, Unity, and Unreal and integrates engine scaffolding, audit, development helpers, and selected install/cache code. It does not claim a universal MagiCore-owned resolver/lockfile for every engine; check the selected engine path before changing package operations.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`engine.rs`](../src/engine.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`dev/mod.rs`](../src/dev/mod.rs)

## Handoff and review

Engine detection and generated starters are not proof of native dependency management. Preserve capability gates and fail-closed behavior for unsupported engines.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-game-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
