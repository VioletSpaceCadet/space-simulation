#!/usr/bin/env bash
# CI gate: Parse batch_summary.json and enforce minimal invariants.
#
# v0: just verify the file exists and is valid JSON with expected fields.
# v1 (future): check thresholds like collapsed_count == 0, min techs_unlocked, etc.
#
# Usage: ./scripts/ci_check_summary.sh [artifacts_dir]
set -euo pipefail

ARTIFACTS="${1:-artifacts}"
SUMMARY="$ARTIFACTS/batch_summary.json"

echo "=== Bench Gate Check ==="

if [ ! -f "$SUMMARY" ]; then
  echo "ERROR: $SUMMARY not found"
  exit 1
fi

# Validate JSON structure
if ! jq -e '.batch_schema_version' "$SUMMARY" > /dev/null 2>&1; then
  echo "ERROR: $SUMMARY is not valid batch_summary JSON"
  exit 1
fi

SCHEMA_VER=$(jq -r '.batch_schema_version' "$SUMMARY")
COLLAPSED=$(jq -r '.collapsed_count' "$SUMMARY")
SEED_COUNT=$(jq -r '.seed_count' "$SUMMARY")
SCENARIO=$(jq -r '.scenario_name' "$SUMMARY")

echo "  scenario:    $SCENARIO"
echo "  seeds:       $SEED_COUNT"
echo "  collapsed:   $COLLAPSED"
echo "  schema:      v$SCHEMA_VER"

# --- v1 gates (enabled) ---
if [ "$COLLAPSED" -gt 0 ]; then
  echo "GATE FAIL: $COLLAPSED seed(s) collapsed"
  exit 1
fi

echo "=== Bench Gate Check passed ==="
