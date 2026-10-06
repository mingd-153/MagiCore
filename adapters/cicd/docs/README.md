# CI/CD Adapter (mgc-cicd-adapter)

<!-- English module guide: purpose, source map, and review boundaries. -->
<!-- Hướng dẫn tiếng Việt về chức năng và ranh giới review của module. -->

## Purpose

Recognizes GitHub Actions, GitLab, CircleCI, Cloudflare, AWS, GCP, and Argo CD markers and provides scaffold/audit integration. Pipeline files remain project-owned; this adapter does not advertise registry dependency resolution, fetch, install, or lockfile ownership.

## Source map

- [`adapter.rs`](../src/adapter.rs)
- [`provider.rs`](../src/provider.rs)
- [`lib.rs`](../src/lib.rs)

## Handoff and review

Keep detection/scaffold/audit separate from pipeline execution or deployment. Check `CicdAdapter::CAPABILITIES`; unsupported package operations must not silently succeed.

## Verification

After code changes, run this crate suite from the repository root with `cargo test -p mgc-cicd-adapter`. For documentation-only changes, check links, language, and Git ignore behavior instead of running unrelated workspace tests.
