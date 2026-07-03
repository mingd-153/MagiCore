# mg — MegaGate Package Manager

Fast, secure, pnpm-style package manager with zero-config scaffolding.

## Quick Start

```bash
# Create pure HTML+CSS+JS project (7 files, zero build tools)
mg create-web my-app

# Create with Vite + TypeScript
mg create-web my-app --vite --ts

# Create full-featured (Vite + TS + Tailwind + Bootstrap + NUI + Sass + API)
mg create-web my-app --vite --ts --tailwind --bootstrap --nui --sass --api

# Install dependencies (if package.json exists)
cd my-app
mg install
```

## Create Web Project

```bash
mg create-web <name> [FLAGS]
```

| Flag | Effect | Auto-enables |
|------|--------|--------------|
| `--vite` | Vite bundler, dev server, HMR | — |
| `--ts` | TypeScript + `tsconfig.json` | `--vite` |
| `--tailwind` | Tailwind CSS + PostCSS | `--vite` |
| `--bootstrap` | Bootstrap CSS (npm dep + CDN) | — |
| `--nui` | Web Components (Button, Card) | `--vite` |
| `--sass` | SCSS support | `--vite` |
| `--api` | `src/services/api.ts` with `fetchJson`/`postJson` | `--vite` |

**Default (no flags)**: Pure HTML+CSS+JS — 7 files, open `index.html` in browser directly.

## Project Structures

### Pure (no flags)
```
my-app/
├── index.html       # HTML entry
├── script.js        # Vanilla JS
├── style.css        # Vanilla CSS
├── .gitignore
├── .env.example
├── .editorconfig
└── README.md
```

### Full (`--vite --ts --tailwind --bootstrap --nui --sass --api`)
```
my-app/
├── package.json           # deps + scripts
├── vite.config.ts         # Vite config
├── tsconfig.json          # TypeScript config
├── tsconfig.node.json
├── tailwind.config.js     # Tailwind config
├── postcss.config.js      # PostCSS config
├── index.html             # HTML entry (root)
├── src/
│   ├── main.ts            # Entry point
│   ├── styles/main.scss   # Styles (SCSS)
│   ├── components/        # Web Components
│   │   ├── app.ts
│   │   ├── button.ts
│   │   └── card.ts
│   ├── services/api.ts    # API client
│   └── utils/helpers.ts   # Utilities
├── public/.gitkeep        # Static assets
├── .gitignore
├── .env.example
├── .editorconfig
└── README.md
```

## Install Dependencies

```bash
mg install [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--hoist` | Hoist deps to project `node_modules/` (default: true) |
| `--offline` | Use cache only, no network |
| `--frozen-lockfile` | Fail if lockfile needs update |
| `--production` | Skip devDependencies |
| `--sandbox` | Run in sandbox mode |

**First install**: Resolves from npm registry, downloads tarballs, extracts to CAS, links via hardlinks + symlinks (~30s for ~25 packages).

**Cached install**: Uses CAS content-addressable store, only creates symlinks (~8s).

## Performance (Apple Silicon)

| Operation | First Run | Cached |
|-----------|-----------|--------|
| Resolve + download | ~30s | N/A |
| Link (linker) | ~4.6ms/6pkgs | ~0.9s total |
| CAS import (1MB) | 3.7ms | — |
| CAS tarball extract | 29ms/100 | — |

## Security

- **CAS hardlinks**: No exposed store paths in `node_modules`, macOS firmlink-safe
- **TOCTOU-safe writes**: Same-file-handle verify on CAS import
- **Path traversal blocked**: `strip_package_prefix` + ancestor checks
- **Symlink attack prevention**: `check_symlink_ancestors` on export, `check_symlink_in_cas` internally
- **Lockfile integrity**: Content hash + deep verification

## Config

Project config: `mgpm.yaml` / `mgpm.yml` / `mgpm.toml` in project root.

```yaml
store:
  store_path: ".mgpm/store"
  virtual_store_path: ".mgpm"
install:
  hoist: true
  concurrency: 16
registries:
  - url: "https://registry.npmjs.org"
```

## Version Resolution

- **New projects**: Latest compatible versions (semver `^` ranges in `package.json`)
- **Existing projects**: Respects existing `package.json` ranges; `mg install` uses lockfile for reproducible installs
- **Registry**: npm (default), configurable via `mgpm.yaml`

## Binary

Built as `mg` (not `mgpm`):

```bash
cargo build -p mgpm-cli --release
# Binary at target/release/mg
```

## License

MIT