# Skills

Domain-specific checklists and guardrails that Claude loads before starting implementation work.

## How It Works

1. Before implementing, Claude scans `.claude/skills/` and matches task keywords against `triggers` in each skill's frontmatter
2. Matched skills are read and their checklists followed
3. If no skills match, `general.md` is used as a fallback
4. A PreToolUse hook (`check-skills.sh`) reminds Claude on the first edit if skills haven't been loaded

## Skill File Format

```markdown
---
name: Human-Readable Name
triggers: [keyword1, keyword2, keyword3]
agents: [agent-name]
---

## When to Use
Brief description of when this skill applies.

## Checklist
- [ ] Step 1
- [ ] Step 2

## Testing
Domain-specific testing guidance.

## Pitfalls
Known gotchas and common mistakes.
```

### Fields

- **name** — Display name for the skill
- **triggers** — Keywords matched case-insensitively against the task description. Use specific terms (file paths, tech names) over generic words
- **agents** — Agent names (from `.claude/agents/`) that should be used when this skill is active

## Adding a New Skill

1. Create a `.md` file in `.claude/skills/`
2. Add YAML frontmatter with `name` and `triggers` (required)
3. Add `agents` if specific agents should be used
4. Write the body with Checklist, Testing, and Pitfalls sections
5. Run `scripts/test_skills.sh` to validate

## Selection Logic

- Match is case-insensitive substring against the task description
- Multiple skills can match simultaneously
- Skills are loaded in alphabetical order
- `general.md` is the fallback when nothing else matches
