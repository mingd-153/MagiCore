# `mgc-lockfile` — Unified Lockfile Engine

This crate provides the unified `mgc.lock` format, integrity metadata, and import support for lockfiles from other package managers (npm, pnpm, yarn, and bun).

## Capabilities

1. **Serialization and deserialization:** reads and writes TOML and JSON representations of `mgc.lock`.
2. **Checksum integrity:** generates and verifies `mgc.lock.sha256`.
3. **Keyed BLAKE3 signatures:** can authenticate a lockfile when `MAGICORE_LOCKFILE_KEY` is configured.
4. **Cross-package-manager import:** supports `package-lock.json` (npm v2/v3), `pnpm-lock.yaml` (pnpm v6/v9), `yarn.lock` (Yarn Classic v1), and `bun.lock` (Bun v1 JSON).

## Usage

### Read and verify a lockfile

```rust
use std::path::Path;
use mgc_lockfile::read_lockfile_checked;

let project_root = Path::new("./my-project");
if let Some(lockfile) = read_lockfile_checked(project_root)? {
    println!("Core: {}, Packages: {}", lockfile.core, lockfile.packages.len());
}
```

### Write a lockfile with its checksum

```rust
use std::path::Path;
use mgc_lockfile::{Lockfile, write_lockfile};

let lockfile = Lockfile::new("web", "frontend");
write_lockfile(Path::new("./my-project"), &lockfile)?;
```

### Import a legacy lockfile

```rust
use std::path::Path;
use mgc_lockfile::import::import_legacy_lockfile_explicit;
use mgc_types::Manifest;

let project_root = Path::new("./legacy-project");
let manifest = Manifest::new("app", mgc_types::Ecosystem::Web);
if let Some(migrated_lock) = import_legacy_lockfile_explicit(
    project_root, "web", "frontend", &manifest
)? {
    println!("Migrated {} packages successfully!", migrated_lock.packages.len());
}
```

## Tests

Run the crate's unit and integration tests with:

```bash
cargo test -p mgc-lockfile
```

See [`READMEVN.md`](READMEVN.md) for the Vietnamese guide.
