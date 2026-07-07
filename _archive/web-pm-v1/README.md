# mg — MegaGate Package Manager

Fast, secure, pnpm-style package manager for Node.js/TypeScript.  
Built with Rust — concurrent, content-addressed, sandbox-ready.

```bash
mg --help          # All commands
mg <command> --help  # Per-command help
```

## Quick Start

```bash
# Create a project
mg create-web my-app          # Vanilla HTML+CSS+JS (zero deps)
mg create-web react my-app    # React SPA (Vite + Router)
mg create-web next my-app     # Next.js App Router

cd my-app

# Install all dependencies (no npm needed)
mg install

# Add/remove packages
mg add zod
mg remove lodash

# Update to latest versions
mg update --latest

# Check what's installed
mg list
mg outdated

# Inspect a package
mg info zod
mg why tslib

# Run scripts
mg run dev
mg run build
```

## Commands

### `mg install` (alias: `mg i`)

Download and link all dependencies from `package.json`.

| Flag | Description |
|------|-------------|
| `--hoist` | Hoist deps to project `node_modules/` |
| `--offline` | Use cache only, no network |
| `--frozen-lockfile` | Fail if lockfile needs update |
| `--production` | Skip devDependencies |
| `--sandbox` | Run in sandboxed mode |

```
mg install
```

Downloads tarballs from registry → extracts to CAS store → links via hardlinks + symlinks → generates `mg.lock`.

---

### `mg add` (alias: `mg a`)

Add a dependency to `package.json` and install it.

```bash
mg add zod                    # Latest, adds with ^
mg add react@18              # Specific major
mg add -D typescript         # devDependencies
mg add --peer react          # peerDependencies
mg add --exact zod           # Lock exact version
mg add zod express           # Multiple packages
```

---

### `mg remove` (alias: `mg rm`)

Remove a dependency from `package.json` and `node_modules`.

```bash
mg remove zod
mg remove lodash uuid
```

---

### `mg outdated`

Compare installed vs latest registry versions.

```bash
mg outdated                   # Only dependencies
mg outdated --dev             # Include devDependencies
```

Sample output:
```
Package                   Installed      Wanted         Latest
-------------------------------------------------------------------
typescript                6.0.3          ^6.0.3         7.0.1-rc
prettier                  3.9.4          ^3.9.4         4.0.0-alpha.9
```

---

### `mg update`

Re-resolve dependencies or bump to latest versions.

```bash
mg update                    # Re-resolve + re-install within current ranges
mg update --latest           # Bump all deps to latest → write package.json → re-install
```

Without `--latest`, re-generates `mg.lock` and reinstalls.  
With `--latest`, fetches each package's latest version from the registry, updates ranges to `^{latest}`, then reinstalls.

---

### `mg info`

Show package metadata from the npm registry.

```bash
mg info zod
```

Sample output:
```
zod
  TypeScript-first schema declaration and validation library

  Latest version:   4.4.3
  License:          MIT
  Versions:         875
  Homepage:         https://zod.dev
  Repository:       git+https://github.com/colinhacks/zod.git
  Maintainers:      colinhacks
  Keywords:         typescript, schema, validation, type, inference
```

---

### `mg why`

Explain why a package is installed (traverse dependency graph).

```bash
mg why tslib
```

Sample output:
```
─ tslib@2.8.1
   └─ Required by:
      ├─ @emnapi/core@1.11.2
      ├─ @emnapi/runtime@1.11.2
      └─ @swc/helpers@0.5.15
```

---

### `mg list` (alias: `mg ls`)

List installed packages from `mg.lock`.

```bash
mg list
```

Sample output:
```
[mg] 166 packages installed

dependencies:
  next 16.3.0-preview.5
  react 19.3.0-canary-fef12a01-20260413
  zod 4.4.3
  ...

146 indirect dependencies
```

---

### `mg link` / `mg unlink`

Link a local package to `node_modules` (symlink) for local development.

```bash
mg link ../my-local-pkg      # Symlinks directory → node_modules/<name>
mg unlink my-local-pkg       # Removes the symlink
```

The linked directory must have a `package.json` with a `name` field.

---

### `mg run`

Run a script defined in `package.json`.

```bash
mg run dev
mg run build
mg run test -- --coverage   # Pass args to the script
mg dev                      # Shorthand for `mg run dev`
```

---

### `mg exec`

Execute a command in the project's context (PATH includes `node_modules/.bin`).

```bash
mg exec tsc -- --noEmit
mg exec jest
```

---

### `mg audit`

Check installed packages for known vulnerabilities.

```bash
mg audit
mg audit --json              # JSON output
mg audit --severity high     # Filter by severity (low, moderate, high, critical)
mg audit --remote            # Fetch latest advisory database
```

---

### `mg init`

Initialize an empty project with `package.json`.

```bash
mg init
```

---

### `mg upgrade`

Upgrade `mg` itself.

```bash
mg upgrade
```

Shows instructions to get the latest release.

---

### `mg create-web`

Scaffold a new web project from templates.

```bash
mg create-web my-app                    # Vanilla HTML+CSS+JS
mg create-web next my-app               # Next.js App Router
mg create-web react my-app --ts         # React + TypeScript
mg create-web vue@3 my-app              # Vue 3
mg create-web my-app --vite --ts --tailwind --bootstrap --nui --sass --api
```

### `mg create-react`

Scaffold a React SPA (Vite + Router + Zustand).

```bash
mg create-react my-app
mg create-react my-app --ts
```

---

### `mg verify`

Verify lockfile integrity.

```bash
mg verify               # Quick check
mg verify --deep        # Walk node_modules, cross-reference with lockfile
```

---

### `mg config`

Manage mg configuration.

```bash
mg config list
mg config set install.hoist true
```

---

### `mg import` / `mg export`

Import from or export to npm-compatible formats.

```bash
mg import package-lock.json format=auto
mg export --output package-lock.json
```

---

## Config

Project config: `mg.yaml` / `mg.yml` / `mg.toml` in project root.

```yaml
store:
  store_path: ".mg/store"
  virtual_store_path: ".mg"
install:
  hoist: true
  concurrency: 16
registries:
  - url: "https://registry.npmjs.org"
```

User config: `~/.config/mg/config.toml`.  
Also reads `.npmrc` from project and home directory.

---

## Architecture

```
package.json → Resolver (PubGrub) → mg.lock (TOML + Bincode)
                                        ↓
                                 Installer
                              ┌────┼────┐
                              ↓    ↓    ↓
                          Store  Cache  Linker
                          (CAS)  (SQLite) (node_modules)
```

- **Resolver**: PubGrub algorithm, dependency confusion protection
- **Lockfile**: Dual format — TOML (readable) + Bincode (fast)
- **Store**: Content-addressable (SHA-256), hardlink + symlink based
- **Linker**: Hoisted mode (default), isolated and PnP modes planned
- **Security**: TOCTOU-safe writes, path traversal protection, integrity hashing

---

## Building from Source

```bash
git clone https://github.com/mingd-153/MegaGate.git
cd web/mg
cargo build --release -p mg-cli
# Binary at target/release/mg
```

Requires Rust 1.84+.

---

## License

MIT
