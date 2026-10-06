# mgc-cache

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Implements a package cache with integrity-aware reads, store/invalidation/pruning, parallel batch helpers, and an in-memory string pool. It is separate from the durable content-addressable store in `mgc-store`; choose the abstraction based on caller lifecycle and ownership.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`memory.rs`](../src/memory.rs)

## Handoff and review

Check cache identity, expected digest matching, and pruning together. A hit is not trusted unless integrity is checked; avoid duplicating `mgc-store` CAS duties.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-cache`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
