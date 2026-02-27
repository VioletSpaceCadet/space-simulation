#!/usr/bin/env bash
# Validates all skill files in .claude/skills/.
# Checks: YAML frontmatter exists, required fields present, referenced agents exist.
# Usage: ./scripts/test_skills.sh

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SKILLS_DIR="$REPO_ROOT/.claude/skills"
AGENTS_DIR="$REPO_ROOT/.claude/agents"
ERRORS=0

if [[ ! -d "$SKILLS_DIR" ]]; then
  echo "FAIL: Skills directory not found: $SKILLS_DIR"
  exit 1
fi

for skill_file in "$SKILLS_DIR"/*.md; do
  filename="$(basename "$skill_file")"

  # Skip README
  if [[ "$filename" == "README.md" ]]; then
    continue
  fi

  echo "Checking $filename..."

  # Check for YAML frontmatter delimiters
  first_line="$(head -1 "$skill_file")"
  if [[ "$first_line" != "---" ]]; then
    echo "  FAIL: Missing YAML frontmatter (no opening ---)"
    ERRORS=$((ERRORS + 1))
    continue
  fi

  # Extract frontmatter (between first and second ---)
  frontmatter="$(sed -n '2,/^---$/p' "$skill_file" | sed '$d')"

  if [[ -z "$frontmatter" ]]; then
    echo "  FAIL: Empty or malformed frontmatter"
    ERRORS=$((ERRORS + 1))
    continue
  fi

  # Check for required 'name' field
  if ! echo "$frontmatter" | grep -q '^name:'; then
    echo "  FAIL: Missing 'name' field in frontmatter"
    ERRORS=$((ERRORS + 1))
  fi

  # Check for required 'triggers' field
  if ! echo "$frontmatter" | grep -q '^triggers:'; then
    echo "  FAIL: Missing 'triggers' field in frontmatter"
    ERRORS=$((ERRORS + 1))
  fi

  # Check referenced agents exist (if agents field present)
  agents_line="$(echo "$frontmatter" | grep '^agents:' || true)"
  if [[ -n "$agents_line" ]]; then
    # Extract agent names from YAML list: [agent1, agent2]
    agents="$(echo "$agents_line" | sed 's/^agents: *\[//;s/\].*//;s/,/ /g')"
    for agent in $agents; do
      agent_name="$(echo "$agent" | tr -d '[:space:]')"
      if [[ -z "$agent_name" ]]; then
        continue
      fi
      agent_path="$AGENTS_DIR/$agent_name.md"
      if [[ ! -f "$agent_path" ]] && [[ ! -d "$AGENTS_DIR/$agent_name" ]]; then
        echo "  WARN: Agent '$agent_name' referenced but not found at $agent_path"
      fi
    done
  fi

  echo "  OK"
done

if [[ $ERRORS -gt 0 ]]; then
  echo ""
  echo "FAILED: $ERRORS error(s) found"
  exit 1
fi

echo ""
echo "All skill files valid."
