# Web scaffold workspace

## Purpose

Groups the intended web-project templates and shared scaffold fragments. Current production lookup is registry-cache based unless a caller explicitly sets `MAGICORE_TEMPLATE_DIR`; the repository path is not the default source. In this checkout, there is no `template.toml` layer or project source/config payload; only descriptive README fragments are present.

## Contents

- `shared/partials/` — placeholder notes for reusable base, Rust-ready, and monorepo scaffold fragments.
- Generators under `scripts/` expect web template paths here when generating template files.

## Status

This README documents the current folder role and the limits evidenced by the checked-in tree. Re-check source, manifests, and tests before relying on a future change.
