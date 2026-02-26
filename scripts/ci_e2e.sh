#!/usr/bin/env bash
set -euo pipefail

echo "=== E2E Tests ==="

# Build daemon
cargo build -p sim_daemon

# Install e2e deps
cd e2e
npm ci
npx playwright install chromium --with-deps

# Run tests
npx playwright test

echo "=== E2E Tests Complete ==="
