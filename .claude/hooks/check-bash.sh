#!/usr/bin/env bash
# PreToolUse hook: catches Bash anti-patterns before they cause problems.
#
# Guards against:
# 1. $() or ${} in gh pr commands — Claude Code blocks command substitution
# 2. "---" or "--flag" — Claude Code blocks quoted characters in flag names
# 3. Long inline --body on gh pr — likely to hit various safety checks
# 4. PATH-prefixed cargo commands — unnecessary and creates approval sprawl
#
# For complex gh pr bodies: use --body-file with a temp file.
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

# --- Check 4: PATH-prefixed cargo commands ---
# cargo is on PATH. Prefixing with PATH= or ~/.cargo/bin creates unique command
# strings that each require separate user approval. Just use "cargo" directly.
if [[ "$CMD" =~ (PATH=|\.cargo/bin/cargo|export\ PATH=).*cargo ]]; then
  echo "STOP: Do not prefix cargo commands with PATH or use ~/.cargo/bin/cargo."
  echo "cargo is already on PATH. Each unique command string requires separate approval."
  echo ""
  echo "Instead of:"
  echo "  PATH=\"\$HOME/.cargo/bin:\$PATH\" cargo test"
  echo "  ~/.cargo/bin/cargo test"
  echo "  export PATH=\"...\" && cargo test"
  echo ""
  echo "Just use:"
  echo "  cargo test"
  echo "  cargo build"
  echo "  cargo clippy"
  echo "  cargo test --manifest-path /path/Cargo.toml   (for worktrees)"
  exit 2
fi
