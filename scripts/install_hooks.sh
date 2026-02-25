#!/usr/bin/env bash
# Install project git hooks by setting core.hooksPath.
# Run once after cloning: ./scripts/install_hooks.sh
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

git config core.hooksPath "$REPO_ROOT/.githooks"
echo "Git hooks installed from .githooks/"
echo "Bypass any hook with: SKIP_HOOKS=1 git commit ..."
