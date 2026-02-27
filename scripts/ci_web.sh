#!/usr/bin/env bash
# CI: Web frontend lint, typecheck, and test suite.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)/ui_web"

echo "=== Web CI ==="

echo "  npm ci..."
npm ci --ignore-scripts

echo "  npm audit..."
npm audit --audit-level=high

echo "  eslint..."
npm run lint

echo "  tsc..."
npx tsc -b --noEmit

echo "  vitest (with coverage)..."
npm run test:coverage

echo "=== Web CI passed ==="
