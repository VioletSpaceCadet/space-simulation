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
# Determine which crate was edited and only test that crate.
# This avoids slow linking of duckdb-dependent crates (sim_cli, sim_daemon)
# when editing unrelated code.
CRATE=""
case "$FILE_PATH" in
  */crates/sim_core/*)    CRATE="sim_core" ;;
  */crates/sim_control/*) CRATE="sim_control" ;;
  */crates/sim_world/*)   CRATE="sim_world" ;;
  */crates/sim_cli/*)     CRATE="sim_cli" ;;
  */crates/sim_daemon/*)  CRATE="sim_daemon" ;;
esac

if [[ -n "$CRATE" ]]; then
  cargo test -p "$CRATE" --quiet 2>&1
else
  # Fallback: test the fast crates only (no duckdb linking)
  cargo test -p sim_core -p sim_control -p sim_world --quiet 2>&1
fi
