# MagiCore template workspace and development input tree

## Purpose

This directory is a development/test override and a target used by template-generation tooling. It is not the production template catalog. The CLI resolves template content from `MAGICORE_TEMPLATE_DIR` when explicitly set, otherwise from the user template cache (`~/.mgc/templates`); `TemplateRoot::resolve` does not fall back to the repository workspace tree.

The preexisting scaffold content consists of seven tracked Markdown fragments under `sources/`; the folder-level README pairs are documentation only. The tree contains no `template.toml` contract and no non-Markdown project source/config payload, so it currently provides **zero publishable template layers** by itself. Empty directories and README fragments do not establish framework support.

This tree is referenced by the E2E harness (`tests/e2e/src/lib.rs`) through `MAGICORE_TEMPLATE_DIR`, by generators such as `scripts/gen-backend.sh` and `scripts/generate-feature-templates.sh`, and by the migration script that recreates its layout. The CI workflow separately extracts a template tree from an earlier Git revision into a temporary directory and runs `mgc template publish-all` against that input. Keep this directory unless those developer/test/generation references are intentionally migrated or removed. Note that `scripts/migrate-to-new-structure.sh` writes a generic `templates/README.md`; rerunning it would overwrite this detailed guide, so reconcile that generator before rerunning it.

## Contents

- [`web/README.md`](web/README.md) / [`web/READMEVN.md`](web/READMEVN.md) — web scaffold subtree and its current limits.
- [`web/shared/README.md`](web/shared/README.md) — shared web fragments.
- [`web/shared/partials/README.md`](web/shared/partials/README.md) — partial layer roles.
- Partial-level guides: [`base`](web/shared/partials/base/README.md), [`frontend Rust-ready`](web/shared/partials/frontend-rust-ready/README.md), [`monorepo Rust-ready`](web/shared/partials/monorepo-frontend-rust-ready/README.md), [`monorepo packages`](web/shared/partials/monorepo-packages/README.md), and [`monorepo`](web/shared/partials/monorepo/README.md).
- The seven files currently under `sources/` are scaffold-facing README content. Keep them as template payload documentation; they are not folder-level instructions for agents.
- `mgc template publish-all` discovers a layer only when it contains both `template.toml` and `sources/`. Neither contract exists in the current checkout.

## Status

This README documents the current folder role and the limits evidenced by the checked-in tree. Re-check source, manifests, and tests before relying on a future change.
