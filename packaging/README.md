# `packaging/` — Distribution Packaging

Pre-built package manager manifests so users can install `mgc` with a single command.

## Structure

```
packaging/
├── homebrew/
│   ├── magicore.rb          # Homebrew formula for macOS/Linux
│   └── magicore-web.rb      # Homebrew formula for single-core web
└── scoop/
    ├── magicore.json        # Scoop manifest for Windows
    └── magicore-web.json    # Scoop manifest for single-core web
```

## Homebrew (`homebrew/`)

For macOS and Linux users via [Homebrew](https://brew.sh/).

```bash
brew install mingd-153/tap/magicore
# or from tap:
brew tap mingd-153/magicore
brew install magicore
brew install magicore-web
```

When releasing a new version, update `magicore.rb` and `magicore-web.rb`:
1. Update the `url` to point to the new release archive.
2. Download the release artifacts locally.
3. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir>`.
4. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only`.
5. Update the `version` field if the release version changed.

## Scoop (`scoop/`)

For Windows users via [Scoop](https://scoop.sh/).

```powershell
scoop bucket add magicore https://github.com/mingd-153/scoop-magicore
scoop install magicore
scoop install magicore-web
```

When releasing a new version, update `magicore.json` and `magicore-web.json`:
1. Update `version`.
2. Update `url` for both x64 and ARM64.
3. Download the release artifacts locally.
4. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir>`.
5. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only`.

## Release Checklist

- [ ] Tag pushed: `git tag v1.0.0-rc.2 && git push origin v1.0.0-rc.2`
- [ ] GitHub Actions `release.yml` builds and attaches all all-core and web-core binary artifacts
- [ ] Homebrew formula updated with `./scripts/update-release-hashes.sh`
- [ ] Scoop manifest updated with `./scripts/update-release-hashes.sh`
- [ ] `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only` passes
