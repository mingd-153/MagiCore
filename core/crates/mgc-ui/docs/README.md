# mgc-ui

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Provides terminal presentation primitives: help, progress bars/spinners, prompts, tables, status/info output, and quiet mode. It owns reusable presentation, not command policy or business decisions.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`help.rs`](../src/help.rs)
- [`progress.rs`](../src/progress.rs)
- [`prompt.rs`](../src/prompt.rs)
- [`table.rs`](../src/table.rs)

## Handoff and review

Keep terminal output English per RULE.md; preserve quiet/non-TTY behavior and avoid stale or misleading success output on errors.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-ui`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
