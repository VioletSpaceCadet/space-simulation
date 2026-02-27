---
name: General
triggers: []
agents: []
---

## When to Use
Fallback for any task that doesn't match a domain-specific skill — docs, config, CI, scripts, or cross-cutting changes.

## Checklist
- [ ] Read relevant existing code before modifying
- [ ] Follow CLAUDE.md conventions (formatting, testing, workflow)
- [ ] Run appropriate tests for the change
- [ ] Update docs (`reference.md`, `CLAUDE.md`) if types or APIs changed
- [ ] No TODO stubs left behind

## Testing
- Rust changes: `cargo test` (runs via PostToolUse hook)
- Frontend changes: `cd ui_web && npm test`
- CI scripts: run the script locally before committing
- Docs: review for accuracy against current code

## Pitfalls
- `cargo` is on PATH — never prefix with `PATH=` or `~/.cargo/bin/`
- Always squash merge, never push directly to main
- PR bodies: use `--body-file /tmp/pr-body.md` not inline `--body`
