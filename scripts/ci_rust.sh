#!/usr/bin/env bash
# CI: Rust format check, clippy lint, and test suite.
set -euo pipefail

echo "=== Rust CI ==="

echo "  cargo fmt --check..."
cargo fmt --check

echo "  cargo clippy..."
cargo clippy -- -D warnings

echo "  cargo test..."
cargo test

echo "=== Rust CI passed ==="
