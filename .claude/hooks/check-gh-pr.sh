#!/usr/bin/env bash
# PreToolUse hook: catches Bash patterns that Claude Code's safety checks block.
# 1. $() or ${} in gh pr commands (command substitution blocked)
# 2. Quoted strings that look like flags, e.g. "---" or "--flag" (blocked everywhere)
set -euo pipefail

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')

if [[ "$TOOL" != "Bash" ]]; then
  exit 0
fi

CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# --- Check 1: $() or ${} in gh pr commands ---
if [[ "$CMD" =~ ^gh\ pr ]]; then
  if [[ "$CMD" =~ \$\( || "$CMD" =~ \$\{ ]]; then
    echo "STOP: gh pr commands cannot use \$() or \${}. Claude Code blocks command substitution."
    echo "Use single-quoted strings instead:"
    echo "  gh pr comment NUMBER --body 'your message here'"
    echo "  gh pr create --title 'title' --body 'body here'"
    echo "For multi-line bodies, use \$'line1\\nline2' or pass --body-file with a temp file."
    exit 2
  fi
fi

# --- Check 2: Quoted strings containing flag-like patterns ---
# Claude Code blocks commands with quoted characters in flag names,
# e.g. echo "---" or echo "--foo". Use unquoted or single-char separators.
if echo "$CMD" | grep -qE "\"--[^\"]*\""; then
  echo "WARNING: Command contains a quoted string that looks like a flag (e.g. \"---\")."
  echo "Claude Code may block this with: 'Command contains quoted characters in flag names'"
  echo "Fix: remove quotes around flag-like strings, or use a different separator."
  echo "  Instead of: echo \"---\"    Use: echo ---  or  echo '====='"
  exit 2
fi
