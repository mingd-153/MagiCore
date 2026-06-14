# Hyper‑Pkg Core Documentation

## 1. Project Overview

**Hyper‑Pkg** is a lightweight, extensible command‑line tool for managing dependencies across many language ecosystems (npm, Cargo, Gradle, …). It provides:

| Feature | Description |
|--------|-------------|
| Unified CLI | One binary (`hyper-pkg`) exposing `install`, `update`, `remove`, `list`, `audit`, `export`, and a terminal UI (`hyper-pkg ui`). |
| Pluggable adapters | Each package manager is implemented as an async `Adapter` trait with `parse` and `update` methods. |
| Shared lock model | A single on‑disk `hyper-pkg.lock` tracks the whole dependency graph, regardless of source manager. |
| Responsive terminal UI | Built with **Ratatui**/Crossterm, loads a logo from a text file, adapts to terminal size, and shows progress bars, menus, and status messages. |
| Future‑proof | Clean separation of **core**, **commands**, **adapters**, and **ui**, making it trivial for downstream agents to extend or replace parts. |

---

## 2. High‑Level Architecture

```
┌─────────────────────────────────────┐
│            hyper-pkg (binary)       │
│  ───►  clap::Parser (CLI entry)─────►│
│  └─────┬─────────────────────────────┘
│        │
│        │  Subcommand selected?
│        ▼
│  ┌─────────────┐          ┌───────────────┐
│  │  Commands   │          │   UI (run_ui) │
│  └───────┬─────┘          └───────┬───────┘
│          │                        │
│          ▼                        ▼
│  ┌─────────────────┐   ┌─────────────────┐
│  │  core::lock.rs │   │  src/ui/mod.rs │
│  └───────┬─────────┘   └───────┬─────────┘
│          │                     │
│          ▼                     ▼
│  ┌─────────────────┐   ┌─────────────────┐
│  │   adapters/    │   │   UI helpers   │
│  │  npm.rs, cargo.rs,…│  │ (logo loader, color gradient, …)│
│  └─────────────────┘   └─────────────────┘
```

* **`src/main.rs`** – parses CLI args with **clap**, dispatches to either a command implementation or the UI (via `run_ui`).
* **`src/commands/`** – async functions (`install`, `update`, `remove`, `list`, `audit`, `export`) that orchestrate the **core** and the chosen **adapter(s)**.
* **`src/core/`** – lock handling (`lock.rs`), generic utilities (`utils.rs`), and optional caching (`cache.rs`).
* **`src/adapters/`** – one module per package manager. Every adapter implements:
```rust
#[async_trait]
pub trait Adapter {
    async fn parse(&self, dir: &str, lock: &mut LockFile) -> Result<Vec<String>>;
    async fn update(&self, dir: &str, pkg: &str) -> Result<()>;
}
```
* **`src/ui/`** – UI entry point (`run_ui`), drawing routine (`draw_ui`), responsive logo loader, and input handling (keyboard & mouse).

---

## 3. Core Components

### 3.1 `LockFile` (`src/core/lock.rs`)
| Field | Type | Meaning |
|------|------|---------|
| `graph` | `HashMap<String, Vec<String>>` | Dependency graph (package → its direct deps). |
| `versions` | `HashMap<String, String>` | Resolved version strings per package. |
| `meta` | `serde_json::Value` | Optional metadata (e.g., timestamps). |

**Key functions**
* `load_lock(path: &Path) -> Result<LockFile>` – deserialises JSON lock file.
* `save_lock(lock: &LockFile, path: &Path) -> Result<()>` – writes lock as pretty‑printed JSON.

The lock file is **the single source of truth** for all adapters; each adapter appends its parsed dependencies into the same `LockFile`.

### 3.2 Command Orchestration (`src/commands/`)
All command functions follow the same pattern:
```rust
pub async fn install(target: Option<String>) -> Result<()> {
    // 1️⃣ Resolve project directory (or use supplied `target`).
    // 2️⃣ Load current lock (or start fresh).
    // 3️⃣ Detect which adapters apply (e.g., Cargo.toml → CargoAdapter).
    // 4️⃣ Call each adapter’s `parse` to fill the lock.
    // 5️⃣ Run the actual package manager (npm install, cargo fetch, …) – out of scope for this core repo.
    // 6️⃣ Persist updated lock.
}
```
* **`update`**, **`remove`**, **`list`**, **`audit`**, **`export`** follow the same skeleton, differing only in the final action (e.g., `audit` runs security checks against the lock).
* All functions return `anyhow::Result<()>`, making error handling uniform across agents.

### 3.3 Adapters (`src/adapters/`)
Each adapter is a **thin wrapper** around the native package manager:
* **CargoAdapter** (`src/adapters/cargo.rs`) – parses `Cargo.toml`, fills lock, optionally runs `cargo update`.
* **NpmAdapter** (`src/adapters/npm.rs`) – parses `package.json`/`package-lock.json`.
* **GradleAdapter** (`src/adapters/gradle.rs`) – parses `build.gradle`/`settings.gradle`.

Adapters are **asynchronous** (via `tokio`) allowing future parallel parsing for large mono‑repos.

**Adding a new package manager**
1. Create `src/adapters/<name>.rs`.
2. Implement the `Adapter` trait.
3. Register the adapter in `src/commands/mod.rs` (or expose via a registry if you move to a plug‑in system).

---

## 4. UI Layer (`src/ui/mod.rs`)

### 4.1 Responsiveness
* **Layout** – uses `ratatui::layout::Layout` with **percentage‑based constraints**:
  * Header = **30 %**
  * Menu = **60 %**
  * Footer = **10 %**
* **Logo handling** – the logo is stored in `src/ui/logo.txt`. The UI:
  1. Reads the file at runtime (`load_logo_lines`).
  2. Truncates each line to the current terminal width (`f.size().width`).
  3. Applies a 4‑step color gradient (`LightCyan → LightMagenta → LightBlue → LightGreen`).
* **Menu** – a list of actions (`MENU_ITEMS`) where each entry has its own color and a visual “selected” indicator (filled circle).
* **Progress bar** – rendered only while a command runs; uses `Gauge` with a custom style and a percentage label (`Span::styled`).
* **Status line** – always displays either “Ready” (white) or the latest message (colored).

### 4.2 Interaction
| Input | Effect |
|------|--------|
| `↑` / `↓` | Move selection in the menu. |
| `Enter` | Execute the highlighted action (calls the async `execute_action`). |
| `q` / `Esc` | Quit UI. |
| Mouse **left‑click** on a menu line | Same as pressing `Enter` on that item. |
| Prompt (`prompt_input`) | Temporarily disables raw mode, prints a colored prompt, reads user input, restores raw mode. |

All UI logic lives in a **single file** (`src/ui/mod.rs`) to keep the UI self‑contained and easy to inherit.

### 4.3 Extending the UI
* Add a new menu entry → update `MENU_ITEMS`.
* Provide a new async handler in `execute_action` that calls the appropriate command (e.g., add a “Clean” option).
* If you need extra UI panels (e.g., a detailed lock‑view), create a new widget and invoke it from `draw_ui`.

---

## 5. Project Structure

**Recommended folder organization for maintainability**

```
hyper-pkg/
├─ Cargo.toml                # package metadata, dependencies (ratatui, clap, colored, async‑trait, anyhow)
├─ src/
│  ├─ main.rs               # CLI entry point, launches UI or commands
│  ├─ commands/             # command implementations (install, update, …)
│  ├─ adapters/             # each package manager lives in its own sub‑directory
│  │  ├─ cargo/             # CargoAdapter + helper code
│  │  │   └─ mod.rs
│  │  ├─ npm/               # NpmAdapter + helper code
│  │  │   └─ mod.rs
│  │  ├─ nuget/             # NuGetAdapter for C# / Unity packages
│  │  │   └─ mod.rs
│  │  ├─ conan/             # optional C++ package manager adapter
│  │  │   └─ mod.rs
│  │  └─ python/            # PythonAdapter (pip) 
│  │      └─ mod.rs
│  ├─ core/                 # lock handling, utils, optional caching
│  ├─ ui/                   # terminal UI implementation (run_ui, draw_ui, logo loader)
│  └─ lib.rs (optional)    # re‑exports for external agents
└─ README.md                # high‑level project description
```

*Each adapter sub‑folder contains its own `mod.rs` (or additional helper files) and implements the `Adapter` trait. This layout keeps language‑specific logic isolated, makes it easy to add or remove adapters, and prevents a single monolithic file from becoming unwieldy.*

---

## 6. Building & Running
```
hyper-pkg/
├─ Cargo.toml                # package metadata, dependencies (ratatui, clap, colored, async‑trait, anyhow)
├─ src/
│  ├─ main.rs               # CLI entry point, launches UI or commands
│  ├─ commands/
│  │   ├─ mod.rs            # public command API (install, update, …)
│  │   └─ <individual command files> (optional)
│  ├─ adapters/
│  │   ├─ cargo.rs
│  │   ├─ npm.rs
│  │   └─ gradle.rs
│  ├─ core/
│  │   ├─ lock.rs           # lock structure + (de)serialization
│  │   ├─ cache.rs          # optional in‑memory/file cache utilities
│  │   └─ utils.rs          # helper functions (e.g., timestamp)
│  ├─ ui/
│  │   ├─ mod.rs            # full UI implementation (+ logo loader, color gradient, …)
│  │   └─ logo.txt          # editable ASCII logo
│  └─ lib.rs (optional)    # re‑exports for external agents
└─ README.md                # high‑level project description
```

---

## 6. Building & Running
```bash
# Build (debug)
cargo build

# Run UI
cargo run -- ui

# Example command usage
cargo run -- install          # install dependencies in the current dir
cargo run -- update <pkg>     # update a specific package
cargo run -- list --graph    # print dependency graph as JSON
```
The UI can be launched at any time; it will automatically detect the working directory and use the **shared lock** if present.

---

## 7. Extending / Forking for Agent‑Based Projects

### 7.1 Adding New Agents
* **Agent goal** – a downstream agent may want to *replace* a part (e.g., a custom UI, a new adapter, or a different lock format).
* **How to replace** – simply **override** the target module by providing a new crate with the same public symbols and update `Cargo.toml` to point to the local path.

### 7.2 Example: Swapping the UI
1. Create a crate `hyper-pkg-ui-custom`.
2. Implement a `run_ui` function compatible with the signature `pub async fn run_ui(project_dir: PathBuf) -> anyhow::Result<()>`.
3. In the root `Cargo.toml`, replace the path for `hyper-pkg`:
```toml
[dependencies]
hyper-pkg = { path = "../hyper-pkg" }
hyper-pkg-ui = { path = "../hyper-pkg-ui-custom" } # new crate
```
4. In `src/main.rs` change the import to `use hyper_pkg_ui::run_ui;`.
The rest of the system (commands, adapters, lock handling) remains untouched.

### 7.3 Adding a New Adapter
1. Add file `src/adapters/<new>.rs`.
2. Implement the `Adapter` trait.
3. Register it in `src/commands/mod.rs` (or a dedicated registry) so `install`/`list` can invoke it.

### 7.4 Testing

---

## 11. Supported Tasks & Application Domains

Hyper‑Pkg is deliberately generic, but it shines in several common development scenarios:

| Domain | Typical workflow | How Hyper‑Pkg helps |
|--------|----------------|----------------------|
| **Web Applications** | Front‑end (npm) + back‑end (Cargo) dependencies | Manage both `package.json` and `Cargo.toml` in a single lock, run a unified `install` to bootstrap the whole stack. |
| **Game Development** | Unity (npm for tooling) + Rust game engine crates | Keep game assets and engine crates synchronized; UI can display progress for large asset pulls. |
| **Micro‑services / Server‑side** | Multiple Rust services, each with its own `Cargo.toml` | Use the lock file to snapshot the entire service mesh dependency graph, audit for vulnerable crates. |
| **CI/CD Pipelines** | Automated builds that need reproducible environments | Export the lock (`hyper-pkg export json`) and feed it into container images for deterministic builds. |
| **Monorepos** | Mixed language repo (JS, Rust, Python) | One command (`hyper-pkg install`) resolves all adapters present, simplifying onboarding for new developers. |
| **Education / Workshops** | Teaching multiple languages in a single repo | Single UI to demonstrate installing, updating, and auditing across languages. |

The UI can be extended with custom panels (e.g., a graph visualiser for the dependency tree) to fit any of these domains.

---

## 12. Supported Build & Package Manager Operations

Hyper‑Pkg is designed to cover the full lifecycle of a project that uses any of the supported package managers. The core commands map directly to the usual build‑tool / package‑manager actions:

| Operation | What Hyper‑Pkg does | Typical underlying command |
|-----------|--------------------|---------------------------|
| **Install** | Reads the manifest(s), resolves dependencies, updates the shared `hyper-pkg.lock`, then runs the native install command (e.g., `npm install`, `cargo fetch`, `pip install -r`). | `npm install`, `cargo fetch`, `pip install -r requirements.txt` |
| **Update** | Fetches the latest compatible version of a specific package (or all packages if none specified) and updates the lock. | `npm update <pkg>`, `cargo update -p <pkg>`, `pip install -U <pkg>` |
| **Remove** | Removes a package from the manifest and lock, then runs the native uninstall command. | `npm uninstall <pkg>`, `cargo remove <pkg>`, `pip uninstall -y <pkg>` |
| **List / Graph** | Prints a consolidated view of all dependencies across adapters, optionally as a JSON graph for tooling. | `npm ls`, `cargo tree`, custom graph output |
| **Audit** | Scans the lock for known vulnerabilities (using advisory databases for each ecosystem). | `npm audit`, `cargo audit`, `safety check` (via external tools) |
| **Export** | Serialises the lock file in different formats (JSON, YAML) for CI pipelines, reproducible builds, or sharing with other tools. | `hyper-pkg export json` |
| **Lock management** | Load, merge, and save the lock file; provides APIs for agents to query versions, graph, or compare snapshots. | Direct file read/write (JSON) |
| **Run custom script** | Delegates to the underlying manager’s script runner (e.g., `npm run <script>`, `cargo run --bin <name>`). | `npm run build`, `cargo run` |
| **Publish / Release** | Calls the native publish command after verifying the lock and version bump. | `npm publish`, `cargo publish` |

These operations are intentionally kept **generic** – any new adapter added later automatically gains the same set of commands as long as it implements the `Adapter` trait.

---

## 8. License & Contributing
All core logic is pure Rust (no external processes). Unit tests can be placed under `src/tests/` or in each module's `#[cfg(test)]` block. Use `cargo test` to verify.

---

## 10. Multi‑Language, Multi‑Core, Multi‑Platform Strategy

- **Multi‑Language** – adapters are language‑specific but all conform to the same `Adapter` trait. Adding support for another language only requires a new adapter module and detection logic; the rest of the core (lock handling, UI, commands) is unchanged.

- **Multi‑Core** – the repository is split into logical cores (`core`, `commands`, `adapters`, `ui`). Each core can be compiled as a separate crate if needed (e.g., `hyper-pkg-core`, `hyper-pkg-adapters`). Conditional compilation (`#[cfg(...)]`) can be used to include OS‑specific code.

- **Multi‑Platform** – the codebase already runs on macOS, Linux, and Windows because:
  * All I/O uses the standard library (`std::fs`, `std::process::Command`).
  * Async runtime (`tokio`) is cross‑platform.
  * UI uses `crossterm`, which abstracts away terminal differences.
  * Platform‑specific adapters can be gated behind `#[cfg(target_os = "windows")]` etc.
  * The lock file format (`JSON`) is portable across OSes.

To extend for a new platform or OS‑specific behaviour, place the code in a module with an appropriate `#[cfg(...)]` attribute and expose it through the public API. The rest of the system will automatically pick the implementation that matches the current target.

---

## 8. License & Contributing
* **License** – MIT (see `LICENSE`).
* **Contributing** – follow the standard GitHub workflow:
  1. Fork the repository.
  2. Create a feature branch (`git checkout -b feat/awesome‑thing`).
  3. Write tests for any new functionality.
  4. Open a Pull Request with a clear description and unit‑test results.

---

## 9. Glossary
| Term | Meaning |
|------|--------|
| **Adapter** | A concrete implementation that knows how to parse & update a specific package manager’s lock files. |
| **LockFile** | Central JSON representation of the dependency graph for the entire project. |
| **UI** | Terminal user interface powered by **Ratatui** and **Crossterm**, fully responsive. |
| **Agent** | A higher‑level automation (e.g., a Claude‑Code or OpenCode Agent) that consumes this core library and possibly extends it. |

---

### TL;DR for agents
* **Core entry point** – `hyper_pkg::run_ui(project_dir: PathBuf)` for UI, or any `hyper_pkg::commands::*` function for CLI actions.
* **Dependency graph** – always stored in `LockFile`; adapters contribute to it, commands read/write it.
* **Extensibility** – add adapters, swap UI, or replace `LockFile` serialization without touching other modules.

Feel free to explore the code; each module is deliberately lightweight to make it straightforward for downstream agents to inherit, modify, or replace components.
