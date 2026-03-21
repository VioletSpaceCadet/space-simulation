---
date: 2026-03-20
topic: dx-tooling-improvements
---

# DX Tooling Improvements: Autonomy & Capabilities

## Problem Frame

The `/implement` and `/project-implementation` workflows are designed for autonomous operation, but several friction points still require human interaction — permission prompts for common commands, banned patterns that force workarounds (temp files for PR bodies, no `&&` chaining), and missing capabilities for design iteration and code review.

## Requirements

### Autonomy Blockers (75% priority)

- R1. **Consolidate allowlist patterns** — Replace the 29 specific `cat > /tmp/pr-*.md << 'DELIMITER'` entries with wildcard patterns. Target: `Bash(cat > /tmp/pr-*.md *)` covers all delimiter variants in one entry.
- R2. **Allow `&&` chaining for safe command families** — Add wildcard allowlist entries for common chains: `git fetch && git diff`, `git checkout && git pull`, `cargo fmt && cargo test`, `cd ui_web && npm test`. These are the most frequent permission prompts during autonomous workflows.
- R3. **Consolidate `gh pr` allowlist patterns** — Replace specific `gh pr merge --squash` / `gh pr create --base` / etc entries with broader wildcards like `Bash(gh pr *)`.
- R4. **Allow `cargo` command variants** — Ensure `cargo test`, `cargo clippy`, `cargo build`, `cargo run`, `cargo fmt`, `cargo llvm-cov` all work without prompts via a single `Bash(cargo *)` pattern.
- R5. **Allow `npm`/`npx` command variants** — Single `Bash(npm *)` and `Bash(npx *)` patterns instead of specific subcommands.
- R6. **Auto-inject subagent file operation instructions** — Instead of manually pasting the "CRITICAL — file operations" block into every task dispatch, add it as a hook or agent preamble so subagents always use Read/Write/Edit tools.

### New Capabilities (25% priority)

- R7. **Wire `ce:review` into `/implement` workflow** — Use Compound Engineering's multi-agent review (`ce:review`) instead of or alongside the basic `pr-reviewer` for deeper analysis on non-trivial PRs.
- R8. **Add `ce:ideate` to project planning** — Before creating tickets, run `ce:ideate` to generate and critically evaluate improvement ideas. Wire into `/project-planner`.
- R9. **Wire `ce:brainstorm` into ticket planning** — For tickets tagged as "needs design" or ambiguous, auto-run `ce:brainstorm` before implementation in the `/implement` flow.

## Success Criteria

- Zero permission prompts during a standard `/implement` run for Rust or FE tickets
- Zero permission prompts during a `/project-implementation` batch run
- Design iteration works end-to-end on UI tickets when `--chrome` is available
- Settings file is clean — no redundant patterns, wildcards where appropriate

## Scope Boundaries

- Not changing Claude Code's safety model — only configuring allowlists within existing settings
- Not building new MCP servers or agents from scratch — leveraging existing Compound Engineering agents
- Not modifying the PreToolUse hooks (`check-bash.sh`) — these catch real anti-patterns. Fixing permissions upstream means the hooks fire less often.
- R6 (auto-inject subagent instructions) may require a hook or agent config change — defer to planning if complex

## Key Decisions

- **Wildcard allowlists over granular patterns**: Simpler maintenance, acceptable risk since these are dev-local commands
- **`ce:review` supplements rather than replaces `pr-reviewer`**: Use `ce:review` for non-trivial PRs (multi-file, new systems), keep basic `pr-reviewer` for small changes
- **Design agents are opt-in via `--chrome` flag**: No degradation when Chrome isn't available

## Outstanding Questions

### Deferred to Planning
- [Affects R6][Technical] Can subagent instructions be injected via a hook, or does it need to be in each agent's `.md` file?
- [Affects R7][Needs research] Does `ce:review` post to GitHub PRs directly, or does it return findings for the orchestrator to post?

## Next Steps

→ `/ce:plan` for structured implementation planning, or proceed directly to work since most items are settings changes.
