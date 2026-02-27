#!/usr/bin/env bash
# PreToolUse hook: reminds Claude to load skills before first edit.
# Warns once per session (marker file in /tmp), then stays silent.
# Exit 0 always — never blocks edits.
#
# Marker is scoped to repo + session start time (hourly granularity).
# Stale markers older than 4 hours are cleaned up automatically.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
REPO_HASH=$(echo -n "$REPO_ROOT" | shasum -a 256 | cut -c1-16)
SESSION_HOUR=$(date +%Y%m%d%H)
MARKER="/tmp/.claude-skills-${REPO_HASH}-${SESSION_HOUR}"

# Clean up stale markers (older than 4 hours)
find /tmp -maxdepth 1 -name ".claude-skills-${REPO_HASH}-*" -not -name "$(basename "$MARKER")" -mmin +240 -delete 2>/dev/null || true

if [[ -f "$MARKER" ]]; then
  exit 0
fi

# Create marker so we only warn once this session
touch "$MARKER"

echo "Skills not loaded yet. Check .claude/skills/ for domain-relevant checklists before coding."
exit 0
