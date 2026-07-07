# MegaGate Core

Shared Rust components used across all adapters.

## Crates

- `mg-http`: HTTP client wrapper
- `mg-store`: Content-addressable store (CAS)
- `mg-crypto`: Integrity verification (SHA-256)
- `mg-lockfile`: Unified lockfile format
- `mg-resolver`: Dependency resolver (PubGrub)
- `mg-fetcher`: Parallel download manager
- `mg-ui`: TUI components (ratatui)
- `mg-config`: Configuration management
- `mg-types`: Shared types, traits, errors

## Development

```bash
cd core
cargo test --all
```
