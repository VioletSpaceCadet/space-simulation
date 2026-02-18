#!/usr/bin/env bash
# Runs after every Edit/Write tool call.
# If the edited file is a .rs file, runs cargo test and cargo fmt.
set -euo pipefail

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only act on Rust source files.
if [[ ! "$FILE_PATH" =~ \.rs$ ]]; then
  exit 0
fi

source "$HOME/.cargo/env" 2>/dev/null || true

REPO_ROOT=$(git -C "$(dirname "$FILE_PATH")" rev-parse --show-toplevel 2>/dev/null)
cd "$REPO_ROOT"

echo "--- fmt ---"
cargo fmt 2>&1

echo "--- test ---"
cargo test --quiet 2>&1
