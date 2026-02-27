#!/usr/bin/env bash
# CI: Rust format check, clippy lint, and test suite.
set -euo pipefail

echo "=== Rust CI ==="

echo "  cargo fmt --check..."
cargo fmt --check

echo "  cargo clippy..."
cargo clippy -- -D warnings

echo "  cargo deny check..."
cargo deny check

IGNORE_RE='tests/|test_helpers|fixtures'

echo "  cargo llvm-cov (test + coverage)..."
cargo llvm-cov --ignore-filename-regex "$IGNORE_RE"

echo "  generating lcov report..."
cargo llvm-cov report --ignore-filename-regex "$IGNORE_RE" --lcov --output-path lcov.info

echo "  checking coverage threshold (83% lines)..."
cargo llvm-cov report --ignore-filename-regex "$IGNORE_RE" --fail-under-lines 83

echo "=== Rust CI passed ==="
