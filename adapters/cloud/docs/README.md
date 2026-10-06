# Cloud Adapter (mgc-cloud-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Classifies CDK, Pulumi, Terraform, and Cloudflare projects and coordinates scaffold/audit plus implemented lifecycle paths. CDK/Pulumi may use an embedded web-backed dependency lane. Terraform package lifecycle and a MagiCore-owned deploy engine are not claimed capabilities.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`cloud_type.rs`](../src/cloud_type.rs)
- [`scaffold/mod.rs`](../src/scaffold/mod.rs)
- [`deploy/mod.rs`](../src/deploy/mod.rs)

## Handoff and review

Trace provider branches and capability lists before claiming deploy or dependency support. A provider CLI passthrough or scaffold is not a MagiCore-owned deployment plan.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-cloud-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
