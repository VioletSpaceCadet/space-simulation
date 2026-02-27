#!/usr/bin/env bash
# PreToolUse hook: catches Bash patterns that Claude Code's safety checks block.
# Claude Code has multiple hardcoded safety checks on Bash commands. This hook
# warns BEFORE the command is sent, so you can fix the approach instead of retrying.
#
# Known blocked patterns:
# 1. $() or ${} — "Command contains $() command substitution"
# 2. "---" or "--flag" — "Command contains quoted characters in flag names"
# 3. Quotes near dashes — "Command contains empty quotes before dash (potential bypass)"
#
# For complex gh pr bodies: ALWAYS use --body-file with a temp file instead of inline strings.
set -euo pipefail

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')

if [[ "$TOOL" != "Bash" ]]; then
  exit 0
fi

CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

BODY_FILE_HINT='For complex PR bodies, use --body-file instead of --body:
  Write the body to a temp file with the Write tool, then:
  gh pr create --title "title" --body-file /tmp/pr-body.md
  gh pr comment NUMBER --body-file /tmp/comment-body.md'

# --- Check 1: $() or ${} in gh pr commands ---
if [[ "$CMD" =~ ^gh\ pr ]]; then
  if [[ "$CMD" =~ \$\( || "$CMD" =~ \$\{ ]]; then
    echo "STOP: gh pr commands cannot use \$() or \${}. Claude Code blocks command substitution."
    echo ""
    echo "$BODY_FILE_HINT"
    exit 2
  fi
fi

# --- Check 2: Quoted strings containing flag-like patterns ---
# Catches "---", "--flag", etc. anywhere in the command.
if echo "$CMD" | grep -qE '"--[^"]*"'; then
  echo "STOP: Command contains a quoted string that looks like a flag (e.g. \"---\")."
  echo "Claude Code blocks this with: 'Command contains quoted characters in flag names'"
  echo ""
  echo "If this is in a gh pr body, use --body-file instead of inline text."
  echo "$BODY_FILE_HINT"
  exit 2
fi

# --- Check 3: gh pr commands with long/complex --body content ---
# If the --body value is longer than 200 chars, it's likely to trigger one of
# Claude Code's safety checks. Recommend --body-file preemptively.
if [[ "$CMD" =~ ^gh\ pr ]]; then
  BODY_CONTENT=$(echo "$CMD" | sed -n "s/.*--body[= ]*'\(.*\)'/\1/p")
  if [[ -z "$BODY_CONTENT" ]]; then
    BODY_CONTENT=$(echo "$CMD" | sed -n 's/.*--body[= ]*"\(.*\)"/\1/p')
  fi
  if [[ ${#BODY_CONTENT} -gt 200 ]]; then
    echo "WARNING: gh pr --body content is long (${#BODY_CONTENT} chars) and likely to trigger Claude Code safety checks."
    echo ""
    echo "$BODY_FILE_HINT"
    exit 2
  fi
fi
