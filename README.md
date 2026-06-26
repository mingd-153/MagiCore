<p align="center"><img src="assets/logo.svg" alt="MegaGate logo" width="900"/></p>

# MegaGate

**MegaGate** – a lightweight, extensible command‑line tool that unifies dependency management across multiple language ecosystems (npm, Cargo, Gradle, Python, etc.).

## Core features
- **Unified CLI** with sub‑commands: `install`, `update <pkg>`, `remove <pkg>`, `list`, `audit`, `export <format>` and a terminal UI (`MegaGate ui`).
- **Pluggable adapters** – each package manager implements the `Adapter` trait (`parse`, `install`, `update`, `remove`). Adding a new manager is just a new module implementing the trait.
- **Global lock file** – `mega-lock.json` stores a consolidated dependency graph (`LockFile`) that aggregates data from all adapters.
- **In-memory agent memory** - the `agent_memory` crate provides a global key/value store for agents (exposed via the `recall` CLI command).
- **Responsive terminal UI** built with **Ratatui**/Crossterm, showing a logo, menu, progress bar and status messages.

## Quick start
```bash
# Build
cargo build

# Install dependencies in the current project (detects the appropriate manager)
mg install

# Update a specific package
mg update <package-name>

# Recall a stored value from the global memory
mg recall my.key

# Launch the interactive UI
mg ui
```

## Extending the core
1. **Add a new adapter** – create `src/adapters/<new>.rs`, implement `Adapter`, and register it in `src/adapters/mod.rs`.
2. **Expose new CLI commands** – extend the `Commands` enum in `src/main.rs` and add a matching async function in `src/commands/mod.rs`.
3. **Custom UI panels** – modify `src/ui/mod.rs` to add new menu items and handling logic.

## License
MIT – see the `LICENSE` file.
