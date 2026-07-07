# MGPM - MegaGate Package Manager

MGPM is a fast, disk-efficient, smart package manager for JavaScript/TypeScript.
Built from scratch in Rust, combining the proven strengths of pnpm, bun, and Vite/Rolldown.

## Key Features

- **Fast**: Parallel downloads, streaming extraction, work-stealing pool
- **Disk-efficient**: Content-addressable store with deduplication
- **Smart**: PubGrub resolver with catalog, workspace, and override support
- **Multi-registry**: npm, JSR, Git, HTTP, file, and workspace protocols
- **Plugin system**: Rollup-compatible hooks, built-in audit/license/size/dep-graph plugins
- **Monorepo-native**: Workspaces, filtering, change detection, recursive commands
