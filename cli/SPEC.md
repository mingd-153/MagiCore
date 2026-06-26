# MegaGate Package Manager - Specification

## 1. Lock File Format: `megagate-lock.json`

```json
{
  "version": 1,
  "packages": {
    "pkg-name@1.2.3": {
      "name": "pkg-name",
      "version": "1.2.3",
      "integrity": "sha512-abc123...",
      "dependencies": {
        "dep-a": "^1.0.0",
        "dep-b": "~2.0.0"
      },
      "optionalDependencies": {},
      "peerDependencies": {},
      "bin": { "cli-name": "./bin/cli.js" },
      "engines": { "node": ">=18" },
      "resolved": "https://registry.npmjs.org/pkg-name/-/pkg-name-1.2.3.tgz",
      "size": 123456
    }
  },
  "importers": {
    ".": {
      "dependencies": {
        "pkg-name": "1.2.3"
      },
      "devDependencies": {},
      "optionalDependencies": {}
    }
  },
  "store": {
    "dir": "~/.megagate/store",
    "layout": "v1"
  }
}
```

### Key Design Decisions
- **Flat package map** (not nested) → O(1) lookup by `name@version`
- **Integrity = SHA-512** của tarball (npm registry standard)
- **Importers** track root workspace + nested workspaces (monorepo ready)
- **Store dir** configurable, default `~/.megagate/store`

---

## 2. Store Layout (Content-Addressable, pnpm-style)

```
~/.megagate/store/
├── v1/
│   ├── files/
│   │   ├── pkg-name-1.2.3.tgz          # Raw tarball (cached)
│   │   └── pkg-name-1.2.3.tgz.sha512   # Integrity file
│   └── nodes/
│       └── pkg-name/
│           └── 1.2.3/
│               ├── package.json        # Extracted package.json
│               ├── node_modules/       # Hardlinked deps (see below)
│               └── .megagate-meta.json # Metadata: { integrity, size, extractedAt }
```

### Node Modules Structure (Hardlink-based, no duplication)

```
project/
├── node_modules/
│   ├── .megagate/                      # Virtual store reference
│   │   └── pkg-name@1.2.3 -> ~/.megagate/store/v1/nodes/pkg-name/1.2.3
│   ├── pkg-name -> .megagate/pkg-name@1.2.3
│   └── dep-a -> .megagate/dep-a@1.0.5
└── megagate-lock.json
```

**Rules:**
- Mỗi `name@version` chỉ tồn tại **1 lần** trong store
- `node_modules/pkg-name` là **symlink** → `.megagate/pkg-name@version`
- `.megagate/pkg-name@version` là **symlink** → store node
- Dependencies bên trong store node cũng là symlink → `.megagate/dep@version`
- **Zero duplication** across projects sharing same store

---

## 3. CLI Interface (Binary: `megagate-pm`)

```bash
# Install all deps from package.json → megagate-lock.json
megagate-pm install [--frozen-lockfile] [--production]

# Add a dependency
megagate-pm add <pkg@version> [--dev] [--optional]

# Update dependencies
megagate-pm update [pkg@version] [--latest]

# Remove dependency
megagate-pm remove <pkg>

# List installed
megagate-pm list [--depth=0] [--json]

# Lock file operations
megagate-pm lock verify          # Verify integrity
megagate-pm lock export [format] # json, yaml

# Store management
megagate-pm store path           # Print store path
megagate-pm store prune          # Remove unreferenced packages
megagate-pm store verify         # Verify all integrity
```

### Exit Codes
- `0` = success
- `1` = generic error
- `2` = lockfile mismatch (frozen-lockfile)
- `3` = integrity verification failed
- `4` = network/registry error

---

## 4. Algorithm: Install (Resolver → Fetcher → Linker)

### 4.1 Resolver
1. Read `package.json` (dependencies + devDependencies + optionalDependencies)
2. If `megagate-lock.json` exists and `--frozen-lockfile`:
   - Use locked versions directly
3. Else:
   - For each dep, query npm registry `/v1/package/<name>/versions` + dist.tarball + dist.integrity
   - Resolve semver ranges → concrete versions
   - Build full dependency graph (transitive)
   - Detect conflicts (same name, different version) → hoist to highest compatible
4. Write `megagate-lock.json`

### 4.2 Fetcher
For each `name@version` in lockfile NOT in store:
1. Download tarball from `resolved` URL (with retry, timeout)
2. Verify SHA-512 integrity
3. Store: `store/files/pkg-name-version.tgz` + `.sha512`
4. Extract to `store/nodes/pkg-name/version/` (streaming, no full extract to temp)

### 4.3 Linker
1. Ensure `project/node_modules/.megagate/` exists
2. For each `name@version` in lockfile:
   - Create symlink: `project/node_modules/.megagate/name@version` → `store/nodes/name/version`
   - Create symlink: `project/node_modules/name` → `.megagate/name@version`
3. For each package in store, link its `dependencies`:
   - `store/nodes/name/version/node_modules/dep` → `../../.megagate/dep@version`
4. Write `.megagate-meta.json` per package

---

## 5. TypeScript Module Structure

```
src/
├── types.ts              # Shared types (LockFile, Package, StoreMeta, etc.)
├── lock.ts               # Load/save/validate megagate-lock.json
├── store.ts              # Store path, layout, integrity verification
├── registry.ts           # npm registry client (fetch metadata, tarballs)
├── resolver.ts           # Version resolution, graph building, conflict detection
├── fetcher.ts            # Download + verify + extract to store
├── linker.ts             # Symlink/hardlink node_modules from store
├── installer.ts          # Orchestrates: resolve → fetch → link
├── cli.ts                # Command parsing (commander), entry point
├── index.ts              # Public API: { install, add, update, remove, list }
└── security/             # Security checks (typosquat, slopsquat, min age, etc.)
```

---

## 6. Integration with Rust Core (New Architecture)

This TypeScript PM will be refactored to consume the Rust core (in `../crates/`) via NAPI-RS bindings.

**New Contract:**
- Rust core (`megagate-core`) provides: resolver, fetcher, linker, store, security
- TypeScript binds to Rust via NAPI-RS: `import { install, add, update, remove, list } from 'megagate-core'`
- Rust handles all core logic; TypeScript provides CLI, developer experience, platform integration

**Migration Path:**
1. Publish Rust core as NAPI-RS npm package
2. Replace TypeScript implementations with Rust bindings
3. Keep TypeScript CLI for developer experience
4. Eventually TypeScript becomes thin wrapper over Rust core
