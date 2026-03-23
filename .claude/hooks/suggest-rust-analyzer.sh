#!/usr/bin/env bash
# PreToolUse hook: suggests rust-analyzer MCP tools when grepping for Rust patterns.
# Never blocks (exit 0 always). Warns once per session.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
REPO_HASH=$(echo -n "$REPO_ROOT" | shasum -a 256 | cut -c1-16)
SESSION_HOUR=$(date +%Y%m%d%H)
MARKER="/tmp/.claude-ra-hint-${REPO_HASH}-${SESSION_HOUR}"

# Only hint once per session hour
if [[ -f "$MARKER" ]]; then
  exit 0
fi

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')

# Only check Grep tool calls
if [[ "$TOOL" != "Grep" ]]; then
  exit 0
fi

PATTERN=$(echo "$INPUT" | jq -r '.tool_input.pattern // empty')
GLOB=$(echo "$INPUT" | jq -r '.tool_input.glob // empty')
FILE_TYPE=$(echo "$INPUT" | jq -r '.tool_input.type // empty')

# Check if this looks like a Rust-specific search
IS_RUST_SEARCH=false

# Searching in .rs files explicitly
if [[ "$GLOB" == *".rs"* ]] || [[ "$FILE_TYPE" == "rust" ]]; then
  IS_RUST_SEARCH=true
fi

# Common patterns that suggest definition/reference lookups
if [[ "$IS_RUST_SEARCH" == "true" ]]; then
  # Patterns that look like struct/fn/trait/enum definitions
  if echo "$PATTERN" | grep -qE '^(struct |fn |trait |enum |impl |pub fn |pub struct |pub trait |pub enum )'; then
    touch "$MARKER"
    echo "Hint: For Rust definition/reference lookups, consider rust_analyzer_definition or rust_analyzer_references — they understand Rust semantics (trait impls, re-exports, generics) where text search cannot."
    exit 0
  fi
fi

exit 0
