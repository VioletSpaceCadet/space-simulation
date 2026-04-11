#!/usr/bin/env bash
# CI: copilot_runtime sidecar lint, typecheck, and test suite.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)/copilot_runtime"

echo "=== copilot_runtime CI ==="

echo "  npm ci..."
npm ci --ignore-scripts

echo "  npm audit..."
# Moderate langsmith transitives from @copilotkit/runtime are not exploitable
# in a localhost-only sidecar. Gate on high+ only.
npm audit --audit-level=high

echo "  eslint..."
npm run lint

echo "  tsc..."
npm run typecheck

echo "  vitest..."
npm test

echo "  tsc build..."
npm run build

echo "=== copilot_runtime CI passed ==="
