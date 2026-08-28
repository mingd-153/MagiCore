# `deploy/` — Deployment & Production Infrastructure

Configuration files for running MagiCore services in production.

## Structure

```
deploy/
├── docker/
│   ├── docker-compose.yml     # Multi-service Docker Compose setup
│   ├── Dockerfile.registry    # Production image for mgc-registry-server
│   └── registry.env.example  # Environment variables template
└── nginx/
    └── magicore.conf          # Nginx TLS reverse proxy configuration
```

## Docker Compose (`docker/`)

Runs two services:

| Service | Description |
|---|---|
| `mgc-registry-server` | Private npm/OCI-compatible package registry |
| `mgc-cli` | Optional CLI sidecar (enable with `--profile cli`) |

### Security Hardening
- Non-root user (`mgreg`, UID 10001)
- Read-only root filesystem
- `tmpfs` for writable temp paths
- No new privileges

### Quick Start
```bash
cp deploy/docker/registry.env.example deploy/docker/.env
# Edit .env with your domain and tokens

docker compose -f deploy/docker/docker-compose.yml up -d
```

## Nginx (`nginx/`)

TLS reverse proxy for `mgc-registry-server`.

**Features:**
- TLS 1.2+ only (TLS 1.3 preferred)
- ACME/Let's Encrypt challenge passthrough (`/.well-known/acme-challenge/`)
- Routes `/npm/*` and `/v2/*` to the upstream registry
- IP-allowlisted admin endpoint (`/v2/admin/*`)
- Upstream keepalive for low-latency package resolution

### Setup
```bash
# Replace YOUR_DOMAIN in magicore.conf with your actual domain
sudo cp deploy/nginx/magicore.conf /etc/nginx/sites-available/magicore
sudo ln -s /etc/nginx/sites-available/magicore /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

## CI/CD Release Pipeline

See `.github/workflows/release.yml` — triggered by a `v*` tag push.

Builds 6 targets:

| OS | Architecture | Artifact |
|---|---|---|
| macOS | Apple Silicon (ARM64) | `magicore-macOS-ARM64.tar.gz` |
| macOS | Intel (x86_64) | `magicore-macOS-X64.tar.gz` |
| Linux | x86_64 | `magicore-Linux-X64.tar.gz` |
| Linux | ARM64 | `magicore-Linux-ARM64.tar.gz` |
| Windows | x86_64 | `magicore-Windows-X64.zip` |
| Windows | ARM64 | `magicore-Windows-ARM64.zip` |

All artifacts include a `.sha256` checksum file.
