#!/usr/bin/env bash
# CI: Run a fast sim_bench smoke scenario and collect artifacts.
# Usage: ./scripts/ci_bench_smoke.sh [output_dir]
#
# sim_bench creates a timestamped subdirectory inside output_dir.
# This script copies the key artifacts up to output_dir for easy upload.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
OUTPUT_DIR="${1:-${REPO_ROOT}/artifacts}"
mkdir -p "$OUTPUT_DIR"

echo "=== Bench Smoke ==="

# Build bench runner (release for realistic perf)
echo "  building sim_bench (release)..."
cargo build -p sim_bench --release --quiet

BENCH="$REPO_ROOT/target/release/sim_bench"

# Run CI smoke scenario (fast: 2000 ticks, 2 seeds)
echo "  running ci_smoke scenario..."
"$BENCH" run --scenario "$REPO_ROOT/scenarios/ci_smoke.json" --output-dir "$OUTPUT_DIR"

# Find the timestamped run directory (most recent)
RUN_DIR=$(find "$OUTPUT_DIR" -maxdepth 1 -type d -name 'ci_smoke_*' | sort | tail -1)

if [ -z "$RUN_DIR" ] || [ ! -f "$RUN_DIR/batch_summary.json" ]; then
  echo "ERROR: batch_summary.json not found in $OUTPUT_DIR/ci_smoke_*/"
  exit 1
fi

# Copy key artifacts to output root for easy CI upload
cp "$RUN_DIR/batch_summary.json" "$OUTPUT_DIR/batch_summary.json"
cp "$RUN_DIR/summary.json" "$OUTPUT_DIR/summary.json" 2>/dev/null || true

echo "  artifacts written to $OUTPUT_DIR/"
echo "  run directory: $RUN_DIR"

echo "=== Bench Smoke passed ==="
