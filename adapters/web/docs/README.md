# Web Adapter (mgc-web-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Implements npm/Node.js package lifecycle: project detection, resolution, registry access, lock checks, secure fetching/materialization, lifecycle policy, cache, audit, list/update, and SBOM. This is the most complete package-manager adapter in this checkout; CLI dispatch and scaffolding are separate layers.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`install/mod.rs`](../src/install/mod.rs)
- [`provider.rs`](../src/provider.rs)
- [`supply_chain.rs`](../src/supply_chain.rs)

## Handoff and review

Trace CLI entry, manifest/lock ownership, capability, and install lane. Preserve integrity, registry URL, lifecycle-script, and stale-cache policies; do not generalize npm guarantees to other cores.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-web-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
