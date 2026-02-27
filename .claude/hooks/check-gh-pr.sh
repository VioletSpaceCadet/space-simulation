#!/usr/bin/env bash
# PreToolUse hook: warns when gh pr commands use $() or ${} which Claude Code blocks.
set -euo pipefail

INPUT=$(cat)
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')

if [[ "$TOOL" != "Bash" ]]; then
  exit 0
fi

CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# Only check gh pr commands
if [[ ! "$CMD" =~ ^gh\ pr ]]; then
  exit 0
fi

# Check for $() or ${} in the command
if [[ "$CMD" =~ \$\( || "$CMD" =~ \$\{ ]]; then
  echo "STOP: gh pr commands cannot use \$() or \${}. Claude Code blocks command substitution."
  echo "Use single-quoted strings instead:"
  echo "  gh pr comment NUMBER --body 'your message here'"
  echo "  gh pr create --title 'title' --body 'body here'"
  echo "For multi-line bodies, use \$'line1\\nline2' or pass --body-file with a temp file."
  exit 2
fi
