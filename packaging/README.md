# `packaging/` — Distribution Packaging

Pre-built package manager manifests so users can install `mg` with a single command.

## Structure

```
packaging/
├── homebrew/
│   ├── megagate.rb          # Homebrew formula for macOS/Linux
│   └── megagate-web.rb      # Homebrew formula for single-core web
└── scoop/
    ├── megagate.json        # Scoop manifest for Windows
    └── megagate-web.json    # Scoop manifest for single-core web
```

## Homebrew (`homebrew/`)

For macOS and Linux users via [Homebrew](https://brew.sh/).

```bash
brew install mingd-153/tap/megagate
# or from tap:
brew tap mingd-153/megagate
brew install megagate
brew install megagate-web
```

When releasing a new version, update `megagate.rb` and `megagate-web.rb`:
1. Update the `url` to point to the new release archive.
2. Download the release artifacts locally.
3. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir>`.
4. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only`.
5. Update the `version` field if the release version changed.

## Scoop (`scoop/`)

For Windows users via [Scoop](https://scoop.sh/).

```powershell
scoop bucket add megagate https://github.com/mingd-153/scoop-megagate
scoop install megagate
scoop install megagate-web
```

When releasing a new version, update `megagate.json` and `megagate-web.json`:
1. Update `version`.
2. Update `url` for both x64 and ARM64.
3. Download the release artifacts locally.
4. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir>`.
5. Run `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only`.

## Release Checklist

- [ ] Tag pushed: `git tag v0.3.0-beta.1 && git push origin v0.3.0-beta.1`
- [ ] GitHub Actions `release.yml` builds and attaches all all-core and web-core binary artifacts
- [ ] Homebrew formula updated with `./scripts/update-release-hashes.sh`
- [ ] Scoop manifest updated with `./scripts/update-release-hashes.sh`
- [ ] `./scripts/update-release-hashes.sh --artifacts <release-assets-dir> --verify-only` passes
