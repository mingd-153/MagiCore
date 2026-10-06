# MagiCore CLI (`mgc`)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Builds the user-facing binary: command definitions, project/core context, bare-command detection, common/per-core dispatch, handlers, and scaffold/bundler integration. CLI coordinates adapters and shared crates; it should not duplicate package-manager behavior owned by those layers.

## Source map

- [`commands/definitions.rs`](../src/commands/definitions.rs)
- [`dispatch/mod.rs`](../src/dispatch/mod.rs)
- [`context.rs`](../src/context.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)

## Handoff and review

For a command change, trace definition → dispatch → handler → shared/core API. Keep bare commands core-neutral and verify workspace/multi-core routing, not only web aliases.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
