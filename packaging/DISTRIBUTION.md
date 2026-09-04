# MagiCore Distribution - Multi-Platform Release Process

## Overview

MagiCore releases artifacts for 6 platforms via GitHub Actions CI:
- macOS ARM64 (Apple Silicon)
- macOS X64 (Intel)
- Linux ARM64
- Linux X64
- Windows ARM64
- Windows X64

## Automated Release Pipeline

### Workflow: `.github/workflows/release.yml`

Triggered on version tags (`v*`):
```bash
git tag v1.1.0-rc.1
git push origin v1.1.0-rc.1
```

### Build Matrix

CI uses native runners for each platform:
- **Linux**: `ubuntu-latest` + `cross` for ARM64
- **macOS**: `macos-latest` (ARM64) + `macos-13` (Intel)
- **Windows**: `windows-latest` (native x64 + ARM64 cross)

### Artifact Outputs

Each platform produces:
- **Unix**: `.tar.gz` archive + `.sha256` file
- **Windows**: `.zip` archive + `.sha256` file

Example:
```
magicore-macOS-ARM64.tar.gz
magicore-macOS-ARM64.tar.gz.sha256
magicore-Windows-X64.zip
magicore-Windows-X64.zip.sha256
```

### Hash Automation

Script: `scripts/update-release-hashes.sh`

**During CI release workflow:**
1. Build all platform artifacts
2. Compute SHA256 for each artifact
3. Auto-update `packaging/homebrew/*.rb` and `packaging/scoop/*.json`
4. Verify hashes match artifacts (`--verify-only`)
5. Upload artifacts + updated manifests to GitHub Release

**Hash format:** 64-character hex string (no `COMPUTED_AFTER_CI_BUILD` placeholders in published manifests)

Example Homebrew:
```ruby
url "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.1/magicore-macOS-ARM64.tar.gz"
sha256 "9f3b9e1e533d86ec77958b06434dcbaf4dabf5fbc17e5011cfaf973daf461413"
```

Example Scoop:
```json
"url": "https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.1/magicore-Windows-X64.zip",
"hash": "a1b2c3d4e5f6..."
```

## Smoke Testing

Script: `scripts/smoke-test.sh`

**Post-release verification (manual or CI):**
```bash
# macOS Homebrew
brew install magicore
./scripts/smoke-test.sh

# Windows Scoop
scoop install magicore
./scripts/smoke-test.sh

# Linux (manual download)
wget https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.1/magicore-Linux-X64.tar.gz
tar -xzf magicore-Linux-X64.tar.gz
./scripts/smoke-test.sh --mgc-path ./mgc
```

**Tests:**
1. `mgc --version` → version string
2. `which mgc` / `where mgc` → binary location
3. `mgc --help` → help output contains "MagiCore"

## Limitations & Requirements

### ✅ Automated
- Builds: All 6 platforms via CI matrix
- Hashes: Auto-computed and embedded in manifests
- Release: GitHub Release with all artifacts

### ⚠️ Manual Steps
- **Tag push**: Developer creates + pushes version tag
- **Post-release smoke test**: Recommended but not automated
- **Homebrew tap update**: Manifests uploaded to release, tap repo needs manual PR (or automation TBD)
- **Scoop bucket**: Similar to Homebrew tap

### 🚫 Not Supported Locally
- **Cross-compilation**: Complex toolchain setup (Linux → Windows, macOS → Linux)
- **Local multi-platform builds**: Use CI for non-native platforms

## Status (v1.1.0-RC)

| Platform | Build | Hash | Smoke Test |
|----------|-------|------|------------|
| macOS ARM64 | ✅ CI | ✅ Auto | ⚠️ Manual |
| macOS X64 | ✅ CI | ✅ Auto | ⚠️ Manual |
| Linux ARM64 | ✅ CI (cross) | ✅ Auto | ⚠️ Manual |
| Linux X64 | ✅ CI | ✅ Auto | ⚠️ Manual |
| Windows X64 | ✅ CI | ✅ Auto | ⚠️ Manual |
| Windows ARM64 | ✅ CI | ✅ Auto | ⚠️ Manual |

**Release readiness:** CI-ready. Hashes auto-updated. No placeholder hashes in published manifests.

**Blocker removed:** Previous placeholder hashes (`COMPUTED_AFTER_CI_BUILD`) replaced with automation-friendly format (`0000...0`) that CI script updates.

## Future Improvements

1. **Automated smoke tests in CI**: Run smoke tests on all platforms post-build
2. **Homebrew tap automation**: Auto-PR to tap repo on release
3. **Scoop bucket automation**: Similar to Homebrew
4. **Artifact signing**: Code signing for macOS/Windows binaries
5. **Checksum verification in install scripts**: Users can verify artifacts independently
