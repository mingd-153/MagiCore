# MagiCore Distribution Packaging

This directory contains package manager manifests and release automation for distributing MagiCore binaries.

---

## Current Status (v1.1.0-rc.1)

### ✅ Ready
- **macOS ARM64**: Real binary + checksum, Homebrew formula updated
- **GitHub Actions**: Full 6-platform build workflow ready
- **Automation**: SHA256 checksum generation, manifest updates

### ⏳ Pending (Requires CI Trigger)
- macOS x86_64 (Intel)
- Linux x86_64
- Linux ARM64
- Windows x86_64
- Windows ARM64

---

## Package Managers

### Homebrew (macOS/Linux)

**Formulas**:
- `homebrew/magicore.rb` - Full distribution (all cores)
- `homebrew/magicore-web.rb` - Web-only distribution

**Status**: 
- ✅ macOS ARM64 checksum: REAL (`9b7593fee3317aea2867075fe17082b427f70213...`)
- ⏳ Other platforms: Placeholder (will be replaced by CI)

**Installation** (after release):
```bash
brew tap magicore/tap
brew install magicore
```

### Scoop (Windows)

**Manifests**:
- `scoop/magicore.json` - Full distribution
- `scoop/magicore-web.json` - Web-only distribution

**Status**: ⏳ All placeholders (needs Windows build)

**Installation** (after release):
```powershell
scoop bucket add magicore https://github.com/mingd-153/MagiCore
scoop install magicore
```

---

## Release Process

### Automated (Recommended)

**Trigger**: Push a version tag

```bash
git tag v1.1.0-rc.1
git push origin v1.1.0-rc.1
```

**GitHub Actions will**:
1. Build binaries for 6 platforms (Linux/macOS/Windows, x86_64/ARM64)
2. Generate SHA256 checksums for all artifacts
3. Update package manager manifests with real checksums
4. Create GitHub Release with all artifacts attached
5. Mark as prerelease if tag contains `alpha`, `beta`, or `rc`

**Timeline**: ~30-60 minutes for all builds

**Artifacts**:
- `magicore-Linux-X64.tar.gz` + `.sha256`
- `magicore-Linux-ARM64.tar.gz` + `.sha256`
- `magicore-macOS-X64.tar.gz` + `.sha256`
- `magicore-macOS-ARM64.tar.gz` + `.sha256`
- `magicore-Windows-X64.zip` + `.sha256`
- `magicore-Windows-ARM64.zip` + `.sha256`
- Web-only variants for each platform

---

### Manual (Local Testing)

**Script**: `scripts/build_all_platforms.sh`

**Usage**:
```bash
./scripts/build_all_platforms.sh [version]
```

**Options**:
1. **GitHub Actions** (recommended) - Instructions to trigger CI build
2. **cross-tool** - Local cross-compilation (requires Docker)
3. **Native only** - Current platform only (quick testing)

**Limitations**:
- Local builds limited by platform (macOS can't build Windows, etc.)
- cross-tool requires Docker setup
- Native build only produces 1 platform artifact

---

## Post-Release Checklist

After GitHub Release is published:

### 1. Download & Verify (5-10 min per platform)

```bash
# Download artifact
curl -LO https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.1/magicore-macOS-ARM64.tar.gz

# Verify checksum
curl -LO https://github.com/mingd-153/MagiCore/releases/download/v1.1.0-rc.1/magicore-macOS-ARM64.tar.gz.sha256
shasum -a 256 -c magicore-macOS-ARM64.tar.gz.sha256

# Extract & test
tar -xzf magicore-macOS-ARM64.tar.gz
./mgc --version
```

Repeat for all 6 platforms (requires VMs/CI for non-native).

### 2. Test Installation (10-15 min per platform)

**macOS**:
```bash
# Fresh system test
brew install magicore
mgc --version
mgc init test-project
cd test-project
mgc install
brew uninstall magicore
```

**Linux**:
```bash
# Manual install test
tar -xzf magicore-Linux-X64.tar.gz
sudo mv mgc /usr/local/bin/
mgc --version
sudo rm /usr/local/bin/mgc
```

**Windows**:
```powershell
# Scoop test
scoop install magicore
mgc --version
mgc init test-project
scoop uninstall magicore
```

### 3. Update Package Repositories

**Homebrew**:
```bash
# Fork homebrew-core
# Update formula with new version + checksums
# Submit PR to Homebrew/homebrew-core

# Or create custom tap
git clone https://github.com/mingd-153/homebrew-magicore
cp packaging/homebrew/magicore.rb homebrew-magicore/Formula/magicore.rb
cd homebrew-magicore
git add Formula/magicore.rb
git commit -m "magicore 1.1.0-rc.1"
git push
```

**Scoop**:
```bash
# Fork scoop bucket
# Update manifest
# Submit PR to ScoopInstaller/Main

# Or create custom bucket
git clone https://github.com/mingd-153/scoop-magicore
cp packaging/scoop/magicore.json scoop-magicore/bucket/magicore.json
cd scoop-magicore
git add bucket/magicore.json
git commit -m "magicore 1.1.0-rc.1"
git push
```

### 4. Announce Release

- Update README.md with installation instructions
- Create release announcement (CHANGELOG.md highlights)
- Post to community channels (if applicable)
- Update documentation with version-specific notes

---

## Troubleshooting

### "Checksum mismatch"

**Cause**: Downloaded file corrupted or tampered

**Solution**: Re-download from GitHub Release, verify source

### "Binary not found after install"

**Cause**: PATH not updated or install location wrong

**Solution** (macOS/Linux):
```bash
which mgc
echo $PATH | grep /usr/local/bin
```

**Solution** (Windows):
```powershell
Get-Command mgc
$env:PATH -split ';' | Select-String scoop
```

### "Permission denied" on macOS

**Cause**: Gatekeeper blocking unsigned binary

**Solution**:
```bash
xattr -d com.apple.quarantine /usr/local/bin/mgc
# Or: System Preferences → Security & Privacy → Click "Allow"
```

### "Cross-compilation fails"

**Cause**: esbuild-rs Go bindings don't cross-compile well

**Solution**: Use GitHub Actions (has native runners for each platform)

---

## Development Notes

### Why Checksums Matter

- **Security**: Verify artifact integrity, detect tampering
- **Package managers**: Homebrew/Scoop require checksums for formula validation
- **Reproducibility**: Ensure same binary for all users

### Platform Targets

| Platform | Arch | Target Triple | Notes |
|----------|------|---------------|-------|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | Most common |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` | Raspberry Pi, AWS Graviton |
| macOS | ARM64 | `aarch64-apple-darwin` | M1/M2/M3 |
| macOS | x86_64 | `x86_64-apple-darwin` | Intel Macs |
| Windows | x86_64 | `x86_64-pc-windows-msvc` | Standard Windows |
| Windows | ARM64 | `aarch64-pc-windows-msvc` | Surface Pro X, Qualcomm |

### Binary Size

- **Unstripped**: ~14 MB (includes debug symbols)
- **Compressed**: ~7 MB (.tar.gz) / ~7.2 MB (.zip)
- **Stripped** (optional): ~8 MB (`strip mgc`)

---

## Future Improvements

- [ ] Code signing (macOS notarization, Windows Authenticode)
- [ ] Apt/Yum repositories (Linux .deb/.rpm packages)
- [ ] Chocolatey support (Windows alternative to Scoop)
- [ ] Snap/Flatpak (universal Linux packaging)
- [ ] Automated smoke tests in CI (post-build validation)
- [ ] Binary size optimization (strip, UPX, feature flags)

---

**Last Updated**: 2026-09-04
**Version**: 1.1.0-rc.1
