# mgc-sbom

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Generates SBOM documents from dependency inventory in supported CycloneDX and SPDX formats. It defines options and component/dependency models and serializes supplied inventory; it does not resolve packages or prove artifact trust.

## Source map

- [`lib.rs`](../src/lib.rs)
- [`generator.rs`](../src/generator.rs)
- [`cyclonedx.rs`](../src/cyclonedx.rs)
- [`spdx.rs`](../src/spdx.rs)

## Handoff and review

Validate identifiers, dependency references, license/version data, and format constraints. Keep provenance/signature verification separate from serialization.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-sbom`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
