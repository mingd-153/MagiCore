# `cli/` — MagiCore Command-Line Interface

The `mgc` binary. All user-facing commands live here.

## Structure

```
cli/src/
├── main.rs               # Entry point — parses Clap args, enters dispatch
├── commands/             # One module per command
│   ├── definitions.rs    # Commands enum (Clap derive) — the single source of all subcommands
│   ├── install.rs        # mgc install
│   ├── audit/            # mgc audit (per-core audit modules)
│   ├── mcp.rs            # mgc mcp — native MCP server (JSON-RPC 2.0 stdio)
│   ├── doctor.rs         # mgc doctor [--fix] — AI-guided environment diagnostic
│   ├── model/            # mgc model push/pull/list (OCI AI model registry)
│   ├── workspace.rs      # mgc workspace
│   └── ...               # All other commands
├── dispatch/
│   ├── engine.rs         # Top-level dispatch: handles --recursive, --filter, workspace loops
│   ├── common.rs         # Routes CommonCommand variants to command handlers
│   ├── per_core.rs       # Maps Commands → DispatchCommand (common or core-specific)
│   ├── bare.rs           # Bare command handling (auto-detect core)
│   └── types.rs          # CommonCommand and CoreCommand enums
├── context.rs            # ProjectContext: reads .mgc.core, mgc.toml, detects ecosystem
└── scaffold/             # Template scaffolding engine (embedded kernel, processors)
```

## How to Add a New Command

1. Add variant to `Commands` in `commands/definitions.rs`.
2. Create `commands/<name>.rs` with a `pub async fn run(...)`.
3. Add `pub mod <name>;` in `commands/mod.rs`.
4. Map the variant in `dispatch/per_core.rs` → `CommonCommand::<Name>`.
5. Add the dispatch arm in `dispatch/common.rs`.
6. Add it to `command_name()` in `dispatch/engine.rs`.
