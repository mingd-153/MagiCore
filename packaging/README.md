# MagiCore Packaging

Packaging configurations for Homebrew and Scoop.

## Current Version

**v1.1.0-rc.1** (Beta Release)

## Updating Release Artifacts

### 1. Build Release Artifacts

```bash
# Build for current platform
cargo build --release -p mgc

# Cross-compile for other platforms (requires cross-rs or CI)
# See .github/workflows/release.yml for full matrix
```

### 2. Compute SHA256 Hashes

```bash
# After building artifacts, compute hashes
./packaging/compute-hashes.sh <release-dir>

# Example output:
# macOS ARM64:
# a1b2c3d4... magicore-macOS-ARM64.tar.gz
```

### 3. Update Formulas

#### Homebrew (`packaging/homebrew/magicore.rb`)

Replace `COMPUTED_AFTER_ARTIFACT_BUILD` with actual SHA256:

```ruby
sha256 "a1b2c3d4e5f6..." # macOS ARM64
```

#### Scoop (`packaging/scoop/magicore.json`)

Replace `COMPUTED_AFTER_ARTIFACT_BUILD` with actual hash:

```json
"hash": "a1b2c3d4e5f6..." // Windows X64
```

### 4. Test Installation

```bash
# Homebrew (local tap)
brew install --build-from-source packaging/homebrew/magicore.rb

# Scoop (local manifest)
scoop install packaging/scoop/magicore.json

# Verify
mgc --version
```

## Release Checklist

- [ ] Build artifacts for all platforms (macOS/Linux/Windows × ARM64/X64)
- [ ] Compute SHA256 hashes (`./packaging/compute-hashes.sh`)
- [ ] Update `magicore.rb` with SHA256
- [ ] Update `magicore.json` with hashes
- [ ] Test install on macOS (Homebrew)
- [ ] Test install on Windows (Scoop)
- [ ] Create GitHub release with artifacts
- [ ] Update public tap/bucket repositories

## Artifact Naming Convention

```
magicore-{OS}-{ARCH}.{ext}

OS: macOS, Linux, Windows
ARCH: ARM64, X64
ext: tar.gz (Unix), zip (Windows)
```

## CI Integration

See `.github/workflows/release.yml` for automated artifact building and hash computation.

## Notes

- **Beta releases** use placeholder `COMPUTED_AFTER_ARTIFACT_BUILD` until CI builds artifacts
- **Production releases** must have real SHA256 hashes before publishing
- Homebrew requires `version` field in formula
- Scoop uses `hash` field (not `sha256`)
