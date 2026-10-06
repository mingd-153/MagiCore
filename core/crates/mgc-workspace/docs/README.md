# mgc-workspace

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Discovers workspace members/catalogs, builds dependency relationships, filters projects, caches derived computation, and returns topological execution levels. CLI consumers schedule these levels; this crate models selection/order rather than executing commands.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`discover.rs`](../src/discover.rs)
- [`graph.rs`](../src/graph.rs)
- [`filter.rs`](../src/filter.rs)
- [`topo.rs`](../src/topo.rs)

## Handoff and review

Check workspace boundary, member identity, edge direction, filters, deterministic order, and CLI handling of partial failures.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-workspace`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
