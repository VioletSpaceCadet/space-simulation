#!/usr/bin/env bash
# CI: Python format check, lint, type check, and test suite.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "=== Python CI ==="

echo "  ruff format --check..."
ruff format --check scripts/analysis/

echo "  ruff check..."
ruff check scripts/analysis/

echo "  mypy..."
mypy scripts/analysis/

echo "  pytest (with coverage)..."
pytest scripts/analysis/tests/ --cov=scripts/analysis --cov-report=term-missing --cov-fail-under=80

echo "=== Python CI passed ==="
