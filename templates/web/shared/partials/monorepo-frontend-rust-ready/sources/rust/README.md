# Engine Crate

This crate is scaffolded by default so the frontend app can grow into a Rust-first architecture
without a destructive refactor later.

Current state:

- no wasm bridge is compiled yet
- no app code depends on this crate by default
- this is the reserved boundary for heavy logic, parsing, transforms, crypto, or local compute
