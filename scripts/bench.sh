#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

BENCH_DIR="adapters/web"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     MegaGate Benchmark Runner                               ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo

MODE="${1:-full}"

case "$MODE" in
  cold)
    echo "── Cold path benchmarks ──"
    cargo bench --bench cold_path --manifest-path "$BENCH_DIR/Cargo.toml"
    ;;
  stress)
    echo "── Stress benchmarks ──"
    cargo bench --bench stress --manifest-path "$BENCH_DIR/Cargo.toml"
    ;;
  install)
    echo "── Install benchmarks ──"
    cargo bench --bench install_bench --manifest-path "$BENCH_DIR/Cargo.toml"
    ;;
  compare)
    echo "── Compare benchmarks ──"
    cargo bench --bench compare --manifest-path "$BENCH_DIR/Cargo.toml"
    ;;
  matrix)
    echo "── Install/materialization matrix ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" --
    ;;
  matrix-heavy)
    echo "── Heavy install/materialization matrix ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --profile heavy
    ;;
  cache-growth)
    echo "── Cache-growth matrix (standard, repeated installs) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --runs 5
    ;;
  cache-growth-heavy)
    echo "── Cache-growth matrix (heavy, repeated installs) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --profile heavy --runs 5
    ;;
  matrix-baseline)
    echo "── Saving matrix baseline (main) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --save-baseline main
    ;;
  matrix-heavy-baseline)
    echo "── Saving heavy matrix baseline (heavy-main) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --profile heavy --save-baseline heavy-main
    ;;
  matrix-diff)
    echo "── Comparing matrix baseline (main) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --compare-baseline main
    ;;
  matrix-heavy-diff)
    echo "── Comparing heavy matrix baseline (heavy-main) ──"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --profile heavy --compare-baseline heavy-main
    ;;
  baseline)
    echo "── Saving baseline (main) ──"
    cargo bench --bench cold_path --manifest-path "$BENCH_DIR/Cargo.toml" -- --save-baseline main
    cargo bench --bench stress --manifest-path "$BENCH_DIR/Cargo.toml" -- --save-baseline main
    echo "Baseline saved. Run './scripts/bench.sh diff' to compare."
    ;;
  diff)
    echo "── Comparing with baseline (main) ──"
    cargo bench --bench cold_path --manifest-path "$BENCH_DIR/Cargo.toml" -- --load-baseline main --baseline-len 50
    cargo bench --bench stress --manifest-path "$BENCH_DIR/Cargo.toml" -- --load-baseline main --baseline-len 50
    ;;
  quick)
    echo "── Quick smoke test (--quick) ──"
    cargo bench --bench cold_path --manifest-path "$BENCH_DIR/Cargo.toml" -- --quick
    cargo bench --bench stress --manifest-path "$BENCH_DIR/Cargo.toml" -- --quick
    cargo bench --bench install_bench --manifest-path "$BENCH_DIR/Cargo.toml" -- --quick
    cargo bench --bench compare --manifest-path "$BENCH_DIR/Cargo.toml" -- --quick
    ;;
  all|full)
    echo "── All benchmarks ──"
    cargo bench --bench cold_path --manifest-path "$BENCH_DIR/Cargo.toml"
    cargo bench --bench stress --manifest-path "$BENCH_DIR/Cargo.toml"
    cargo bench --bench install_bench --manifest-path "$BENCH_DIR/Cargo.toml"
    cargo bench --bench compare --manifest-path "$BENCH_DIR/Cargo.toml"
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" --
    cargo run --bin bench_matrix --manifest-path "$BENCH_DIR/Cargo.toml" -- --profile heavy
    ;;
  *)
    echo "Usage: $0 [cold|stress|install|compare|matrix|matrix-heavy|cache-growth|cache-growth-heavy|matrix-baseline|matrix-heavy-baseline|matrix-diff|matrix-heavy-diff|baseline|diff|quick|all]"
    exit 1
    ;;
esac
