# Web Template Design Guide

## Architecture

Each template = directory with `template.toml` + `sources/` subdirectory.

```
templates/web/<mode>/<path>/
├── template.toml     # File manifest with feature gating
└── sources/          # Source files (substituted when scaffolded)
    ├── package.json  # NPM package manifest (uses {{project_slug}})
    ├── index.ts.html # Vite HTML entry (uses {{project_name}})
    └── ...
```

## template.toml Schema

```toml
[[files]]
source = "relative/path/in/sources"     # source path inside sources/ dir
target = "relative/output/path"          # target in scaffolded project
required_context = ["project_slug"]      # context vars needed (optional)
include_features = ["typescript"]        # only when feature present (optional)
exclude_features = ["typescript"]        # only when feature absent (optional)
```

## File Selection Rules

- **JS/TS dual**: same file with `include/exclude_features = ["typescript"]`
  - JS variant: `exclude_features = ["typescript"]`
  - TS variant: `include_features = ["typescript"]`
- **Always included**: no feature gates
- **Context required**: `required_context` — fills {{mustache}} templates

## Shared Partials

Partials under `shared/partials/` are layered on top of framework templates.
Each partial = its own `template.toml` + `sources/`.

| Layer | Purpose |
|-------|---------|
| `base/` | Root: README, .gitignore, mg.lock, web.toml |
| `frontend-common/` | AppShell component |
| `frontend-foundation/` | Theme, favicon, brand config |
| `frontend-rust-ready/` | Runtime bridge, Cargo crate |
| `frontend/` | Site content pages |
| `backend/` | Config, health routes, status service |
| `monorepo/` | Root monorepo setup |
| `monorepo-frontend-common/` | AppShell in monorepo |
| `monorepo-frontend-foundation/` | Theme in monorepo |
| `monorepo-frontend-rust-ready/` | Bridge in monorepo |
| `monorepo-frontend/` | Site content in monorepo |
| `monorepo-backend/` | BE config in monorepo |
| `monorepo-packages/` | Shared packages |

## Adding a New Framework

### Frontend
1. Add `FRAMEWORK_SEEDS` entry in `cli/src/commands/core/web.rs`
2. Create `templates/web/frontend/<name>/template.toml` + sources
3. Create `templates/web/monorepo/frontend/<name>/template.toml` + sources
4. Add alias in `normalize_cli_web_framework()` if needed
5. Add to `resolve_seed_name()` if used in fullstack combo

### Backend
1. Create `templates/web/backend/<lang>/<name>/template.toml` + sources
2. Already registered in `WEB_FRAMEWORKS` with `base: Some("<lang>")`

### Fullstack
1. Create `templates/web/fullstack/<mode>/<name>/template.toml` + sources
2. Add to `resolve_seed_name()` mapping
3. Add to `fullstack_backend_framework()` mapping

## Template Variables

| Variable | Source | Example |
|----------|--------|---------|
| `{{project_slug}}` | CLI --name | `my-app` |
| `{{project_name}}` | Derived from slug | `My App` |
| `{{backend_framework}}` | Fullstack combo | `fastify` |
