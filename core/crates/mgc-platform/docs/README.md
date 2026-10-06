# mgc-platform

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides current low-level cross-platform filesystem APIs: standard paths, a bounded filesystem-write semaphore, and reflink support. Its public module list is narrower than some historical descriptions; do not assume shell, permission, or symlink APIs without checking source.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`paths.rs`](../src/paths.rs)
- [`fs_semaphore.rs`](../src/fs_semaphore.rs)
- [`reflink.rs`](../src/reflink.rs)

## Handoff and review

Keep fallback behavior explicit when reflink is unavailable. Verify path behavior per OS and ensure new writes respect the shared concurrency limit.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-platform`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
