# Getting Started

## Installation

### macOS (Homebrew)

```bash
brew install megagate/tap/mgpm
```

### Shell script (Linux/macOS)

```bash
curl -fsSL https://mgpm.dev/install.sh | sh
```

### npm

```bash
npm install -g @megagate/mgpm
```

### From source

```bash
git clone https://github.com/megagate/mgpm.git
cd mgpm
cargo build --release
# Binary is at target/release/mgpm-cli
```

## Quick start

```bash
# Initialize a new project
mgpm init

# Add a dependency
mgpm add lodash

# Install all dependencies
mgpm install

# Run a script
mgpm run build
```

## Next steps

- Read the [CLI Reference](cli-reference.md) for all commands
- Learn about [Configuration](configuration.md) options
- Explore [Workspaces](workspaces.md) for monorepo setups
