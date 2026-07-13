# MegaGate Usage

## Local bootstrap builds

```bash
# Full multi-core distribution
./scripts/build.sh megagate

# Single-core web distribution
./scripts/build.sh megagate-web

# Every packaged distribution
./scripts/build.sh all
```

Artifacts:

```bash
dist/megagate/<target>/mg
dist/megagate-web/<target>/mg
```

## Run the built binaries

### Full build

```bash
./dist/megagate/<target>/mg --help
./dist/megagate/<target>/mg create-web react@latest my-app --ts --tailwindcss
```

### Web-only build

```bash
./dist/megagate-web/<target>/mg --help
./dist/megagate-web/<target>/mg create react@latest my-app --ts --tailwindcss
```

## Notes

- full build = multi-core command surface
- web-only build = single-core web command surface
- packaged manifests now exist for all core distributions
