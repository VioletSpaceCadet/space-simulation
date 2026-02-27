#!/usr/bin/env bash
# PreToolUse hook: reminds Claude to load skills before first edit.
# Warns once per session (marker file in /tmp), then stays silent.
# Exit 0 always — never blocks edits.

set -euo pipefail

# Derive a stable marker from the repo root path
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
REPO_HASH=$(echo -n "$REPO_ROOT" | shasum -a 256 | cut -c1-16)
MARKER="/tmp/.claude-skills-loaded-${REPO_HASH}"

if [[ -f "$MARKER" ]]; then
  exit 0
fi

# Create marker so we only warn once
touch "$MARKER"

echo "Skills not loaded yet. Check .claude/skills/ for domain-relevant checklists before coding."
exit 0
