#!/usr/bin/env bash
# CI: Rust format check, clippy lint, and test suite.
set -euo pipefail

echo "=== Rust CI ==="

echo "  cargo fmt --check..."
cargo fmt --check

echo "  cargo clippy..."
cargo clippy -- -D warnings

echo "  cargo llvm-cov (test + coverage, fail-under-lines 83)..."
cargo llvm-cov --ignore-filename-regex 'tests/|test_helpers|fixtures' --fail-under-lines 83

echo "  generating lcov report..."
cargo llvm-cov report --ignore-filename-regex 'tests/|test_helpers|fixtures' --lcov --output-path lcov.info

echo "=== Rust CI passed ==="
