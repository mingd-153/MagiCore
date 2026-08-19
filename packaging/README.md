# `packaging/` — Distribution Packaging

Pre-built package manager manifests so users can install `mg` with a single command.

## Structure

```
packaging/
├── homebrew/
│   └── megagate.rb          # Homebrew formula for macOS/Linux
└── scoop/
    └── megagate.json        # Scoop manifest for Windows
```

## Homebrew (`homebrew/`)

For macOS and Linux users via [Homebrew](https://brew.sh/).

```bash
brew install mingd-153/tap/megagate
# or from tap:
brew tap mingd-153/megagate
brew install megagate
```

When releasing a new version, update `megagate.rb`:
1. Update the `url` to point to the new release archive.
2. Update the `sha256` hash (download the file and run `sha256sum`).
3. Update the `version` field.

## Scoop (`scoop/`)

For Windows users via [Scoop](https://scoop.sh/).

```powershell
scoop bucket add megagate https://github.com/mingd-153/scoop-megagate
scoop install megagate
```

When releasing a new version, update `megagate.json`:
1. Update `version`.
2. Update `url` for both x64 and ARM64.
3. Update `hash` values.

## Release Checklist

- [ ] Tag pushed: `git tag v0.3.0-beta.1 && git push origin v0.3.0-beta.1`
- [ ] GitHub Actions `release.yml` builds and attaches all 6 binary artifacts
- [ ] Homebrew formula updated with new SHA256
- [ ] Scoop manifest updated with new SHA256
