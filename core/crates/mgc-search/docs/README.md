# mgc-search

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Coordinates implemented npm, crates.io, Go, and PyPI discovery clients; normalizes results, ranks candidates, caches queries, and supports prompting. Search results are discovery data, not a resolved or trusted install plan.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`orchestrator.rs`](../src/orchestrator.rs)
- [`clients/mod.rs`](../src/clients/mod.rs)
- [`ranking.rs`](../src/ranking.rs)
- [`types.rs`](../src/types.rs)

## Handoff and review

Check source attribution, pagination/rate limits, ranking, and cache freshness. Install resolution/trust belongs in resolver/adapter flows; ranked results are not authority.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-search`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
