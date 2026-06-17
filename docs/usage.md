# MegaGate – Interactive UI

`Megagate ui` launches a tiny terminal‐based menu that lets you perform the most common actions without remembering flags.

## Quick start
```bash
# Build (if you haven't already)
cargo build --release

# Run UI (in the root of your multi‑language project)
./target/release/Megagate ui
```

### What the UI does
1. **Ask for the project directory** – default is the current folder (`.`).
2. **Detect the package manager** (npm/pnpm/bun, Cargo, Gradle, …) automatically.
3. **Present a menu** with the following options:
   - Install dependencies
   - Update a specific package
   - Remove a package
   - List dependencies (graph – Mermaid output)
   - Run security audit
   - Export the lock file to a chosen format
   - Exit
4. After the chosen operation finishes, the UI will:
   - Write/update `mega-lock.json`
   - (Placeholder) resolve any new artifacts in the content‑addressable cache.
5. The program then exits cleanly.

## Screenshots (textual)
```
🤖 MegaGate Interactive UI

Project directory (default: .): /path/to/your/project
Detected package manager: npm/pnpm/bun

? Choose an action  [Use arrows key]
> Install dependencies
  Update a package
  Remove a package
  List dependencies (graph)
  Run audit
  Export lock file
  Exit
```

The UI is built with the **dialoguer** crate, which works on macOS and Linux out of the box. No extra graphical libraries are required.

## When to use the UI vs. the CLI
- **UI** – ideal for occasional developers, demos, or when you just want a guided experience.
- **CLI** – perfect for scripts, CI pipelines, or when you already know the exact flags you need.

Both interfaces share the same internal implementation, so results are identical.
