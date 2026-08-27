# MagiCore Benchmark Suite

Reproducible benchmark comparing MagiCore vs pnpm/Bun/npm on JavaScript PM performance.

## Environment

- **OS:** Ubuntu 22.04 (Docker container)
- **Node:** 20.x LTS
- **Rust:** 1.96.0 stable
- **Package Managers:**
  - MagiCore: `mgc` (built from source)
  - pnpm: latest (npm global install)
  - Bun: latest (official installer)
  - npm: bundled with Node 20

## Test Package

- **Project:** Typical React + Next.js application
- **Dependencies:** 50 packages (29 runtime + 21 dev)
- **Stack:** React 18, Next.js 14, TypeScript, Tailwind CSS, Radix UI
- **File:** `env/package.json`

## Structure

```
benchmark/
├── env/
│   ├── Dockerfile          # Clean Ubuntu 22.04 + Node 20 + PMs
│   └── package.json        # Test package (50 deps)
├── scripts/
│   └── run_benchmark.sh    # Benchmark runner (cold/warm installs)
├── results/                # JSON outputs (gitignored)
│   └── *.json
├── docker-compose.yml      # Orchestration
└── README.md               # This file
```

## Usage

### 1. Build Docker image

```bash
cd benchmark
docker-compose build
```

### 2. Start container

```bash
docker-compose up -d
docker exec -it magicore-benchmark bash
```

### 3. Run benchmarks

Inside container:

```bash
# MagiCore (5 runs)
for i in {1..5}; do
  /benchmark/scripts/run_benchmark.sh mgc $i
done

# pnpm (5 runs)
for i in {1..5}; do
  /benchmark/scripts/run_benchmark.sh pnpm $i
done

# Bun (5 runs)
for i in {1..5}; do
  /benchmark/scripts/run_benchmark.sh bun $i
done

# npm (5 runs)
for i in {1..5}; do
  /benchmark/scripts/run_benchmark.sh npm $i
done
```

### 4. Results

JSON files saved to `results/`:
- `mgc_run1_<timestamp>.json`
- `pnpm_run1_<timestamp>.json`
- etc.

Each result contains:
```json
{
  "pm": "mgc",
  "run": 1,
  "timestamp": "20260827_104530",
  "machine": {
    "cpu": "...",
    "cores": 4,
    "memory_gb": 8,
    "os": "Linux ...",
    "node_version": "v20.x.x"
  },
  "cold_install": {
    "duration_seconds": 5.234,
    "disk_mb": 147,
    "memory_delta_mb": 512
  },
  "warm_install": {
    "duration_seconds": 1.823
  },
  "package_count": 50
}
```

## Analysis

After collecting results, run analysis script (Week 2):
```bash
python analyze.py results/*.json > BENCHMARK.md
```

## Reproduce

To reproduce benchmark on your machine:
1. Ensure Docker installed
2. Clone MagiCore repo
3. Build MagiCore release: `cargo build --release`
4. Run: `cd benchmark && docker-compose up -d`
5. Execute benchmarks inside container
6. Results written to `benchmark/results/`

## Exit Criteria (Gate 1)

- ✅ Benchmark artifact exists (raw JSON + BENCHMARK.md + reproduce.sh)
- ✅ MagiCore matches pnpm speed (±20%) OR honest explanation why not
- ✅ Disk usage 9.2MB claim verified OR corrected with real number
- ✅ Monorepo 100+ packages tested (no crash, performance measured)
- ✅ Reproducible (run 3 times, results ±5%)

## Notes

- Clean state enforced (cache cleared before each cold run)
- Warm runs use PM's cache (realistic developer workflow)
- Resource limits: 4 CPUs, 8GB RAM (fair comparison)
- Statistical analysis: mean/median/std dev from 5 runs
