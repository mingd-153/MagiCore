# mgc-store

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Implements the durable package store around CAS, integrity, lifecycle/cache, database/index, and install/write coordination. It includes generation/lease state and failure-injection hooks, and is distinct from the lighter `mgc-cache`.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`cas/mod.rs`](../src/cas/mod.rs)
- [`database.rs`](../src/database.rs)
- [`index.rs`](../src/index.rs)
- [`layout.rs`](../src/layout.rs)

## Handoff and review

Review crash consistency, concurrent writes, lease recovery, references, integrity, and migrations together. Do not turn verification failures into misses or silent repairs without evidence.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-store`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
