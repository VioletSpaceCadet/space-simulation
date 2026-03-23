#!/usr/bin/env bash
# SessionStart / SubagentStart hook: comprehensive session briefing.
# Injects additionalContext with key docs, tools, and practices.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Build the context as a variable for readability.
# Use \n for newlines in JSON string.
read -r -d '' CONTEXT <<'HEREDOC' || true
## Session Briefing

### Rust Analyzer MCP (prefer over grep for Rust navigation)
Call `rust_analyzer_set_workspace` with the repo root before first use (especially in worktrees).
- `rust_analyzer_definition` — go-to-definition across crates. Use instead of grepping for struct/fn/trait definitions.
- `rust_analyzer_references` — find all usages of a symbol across workspace. Semantically accurate.
- `rust_analyzer_hover` — full type signature at a position. Quick check without reading surrounding code.
- `rust_analyzer_symbols` — file outline (functions, structs, consts with line ranges). Use for "what's in this file?"
- `rust_analyzer_diagnostics` — per-file native RA diagnostics (type mismatches, unresolved imports). Does NOT include cargo/rustc/clippy — always run `cargo check` for full validation.
All positions are 0-based (LSP convention). Subtract 1 from Read tool line numbers.
Avoid: `workspace_diagnostics` (unsupported by RA, returns empty), `completion` (250K+ output, no filtering), `format` (redundant with cargo fmt hook).

### Key Documentation — read before modifying related areas
- `docs/DESIGN_SPINE.md` — Authoritative design philosophy. Read before proposing architectural changes.
- `docs/reference.md` — Detailed types, content file schemas, inventory/refinery design. Read when touching GameState, content loading, or module types.
- `docs/workflow.md` — CI checks, hook behavior, PR conventions, scenario testing, balance tuning loop.
- `docs/BALANCE.md` — Balance analysis findings and tuning decisions.
- `docs/solutions/` — Past debugging solutions and pattern discoveries. Check before debugging similar issues.
- `docs/plans/` — Historical design and implementation plans. Check for context on how a system was designed.
- `content/knowledge/playbook.md` — Living strategy doc. Check via `query_knowledge` MCP tool before balance analysis.

### Doc routing by task type
- **Sim core changes** (tick, GameState, modules): Read `reference.md` + `DESIGN_SPINE.md`
- **Balance/tuning work**: Read `BALANCE.md`, call `query_knowledge`, check `docs/plans/` for system design
- **New feature/system**: Check `docs/plans/` for prior design docs on related systems
- **Bug fix**: Check `docs/solutions/` for past fixes in the same area
- **Frontend work**: Read `reference.md` (API contract), check `docs/plans/` for FE design docs
- **CI/workflow issues**: Read `docs/workflow.md`

### Skills system
Check `.claude/skills/` for domain-specific checklists before coding. Key skills:
- `rust-sim-core.md` — sim_core, sim_world, sim_control (determinism, tick order, content-driven types)
- `rust-daemon.md` — sim_daemon (HTTP, SSE, async safety, axum)
- `frontend-dev.md` — ui_web (React, Tailwind, panels, SSE hooks)
- `cross-layer-e2e.md` — changes spanning FE + daemon
- `balance-analysis.md` — simulation analysis and tuning
HEREDOC

# Escape for JSON: replace newlines with \n, escape quotes
JSON_CONTEXT=$(echo "$CONTEXT" | jq -Rs .)

cat <<ENDJSON
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": ${JSON_CONTEXT}
  }
}
ENDJSON
