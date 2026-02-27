# CLAUDE.md

Space industry simulation game. Deterministic Rust sim core, HTTP daemon with SSE event streaming, React mission control UI.

`docs/DESIGN_SPINE.md` — authoritative design philosophy. `docs/reference.md` — detailed types, content files, inventory/refinery design. `docs/workflow.md` — CI, hooks, PR conventions, scenarios, balance tuning loop. `base-project.md` — original design doc. Balance analysis tracked in Linear ("Balance & Tuning" project, VioletSpaceCadet workspace).

## Common Commands

```bash
cargo build                                               # Build all crates
cargo test                                                # Run all tests
cargo test -p sim_core                                    # Test sim_core only
cargo test <name>                                         # Run a single test by name
cargo clippy                                              # Lint
cargo fmt                                                 # Format

cargo run -p sim_cli -- run --ticks 1000 --seed 42        # CLI runner
cargo run -p sim_cli -- run --state content/dev_base_state.json
cargo run -p sim_daemon -- run --seed 42                  # HTTP daemon (:3001)
cd ui_web && npm run dev                                  # React UI (:5173)
cd ui_web && npm test                                     # vitest

cargo run -p sim_bench -- run --scenario scenarios/baseline.json

cd mcp_advisor && npm run build                           # Build MCP advisor
cd mcp_advisor && npm start                               # Run MCP advisor (stdio transport)

cd e2e && npx playwright test                             # E2E tests
cd e2e && npx playwright test --headed                    # E2E tests (visible browser)

cargo llvm-cov --fail-under-lines 83                      # Rust coverage (83% line threshold)
cd ui_web && npm run test:coverage                        # FE coverage (thresholds in vite.config.ts)

./scripts/ci_rust.sh                                      # fmt + clippy + test + coverage
./scripts/ci_web.sh                                       # npm ci + lint + tsc + vitest + coverage
./scripts/ci_bench_smoke.sh                               # Release build + ci_smoke scenario
./scripts/ci_e2e.sh                                       # E2E Playwright tests
./scripts/ci_event_sync.sh                                # Event exhaustiveness check
```

## Architecture

Cargo workspace: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`. Plus `sim_world` (content loading + world gen) and `ui_web/` (React).

- **sim_core** — Pure deterministic sim. No IO. Public API: `tick()`, `inventory_volume_m3()`, `mine_duration()`, etc.
- **sim_control** — `AutopilotController` (deposit→mine→deepscan→survey priority + station module auto-management).
- **sim_world** — `load_content()` + `build_initial_state()`. Content from `content/*.json`.
- **sim_bench** — Scenario runner. JSON overrides (constants + `module.*` dotted keys). Parallel seeds via rayon.
- **sim_cli** — CLI tick loop with autopilot. `--state`, `--metrics-every`, `--no-metrics` flags.
- **sim_daemon** — axum 0.7, SSE, AlertEngine, pause/resume, command queue. See `docs/reference.md` for endpoints. Includes `analytics` module (trend/rate/bottleneck analysis) and `GET /api/v1/advisor/digest` endpoint.
- **mcp_advisor** — MCP server (TypeScript, stdio transport) for balance analysis. Auto-discovered via `.mcp.json`. Requires running `sim_daemon`.
- **e2e** — Playwright E2E smoke tests. Global setup spawns daemon (port 3002) + Vite (port 5174). Kept minimal for CI stability; use Chrome browser tools for ad-hoc UI testing.
- **ui_web** — Vite 7 + React 19 + TS 5 + Tailwind v4. Draggable panels, SSE streaming, keyboard shortcuts.

**Tick order:** 1. Apply commands → 2. Resolve ship tasks → 3. Tick station modules (processors, assemblers, sensors, labs, maintenance) → 4. Advance research → 5. Replenish scan sites → 6. Increment tick.

**Key design rules:**
- Asteroids created on discovery (scan_sites → AsteroidState), not pre-populated.
- Research uses lab-based domain system. Labs consume raw data, produce domain-specific points. Tech unlock is probabilistic.
- Raw data is sim-wide (on ResearchState), not station inventory. Rolls every N ticks, not every tick.
- DeepScan commands dropped if no unlocked tech has EnableDeepScan effect.
- All collection iteration sorted by ID before RNG use for determinism.
- sim_core takes `&mut impl rand::Rng` — concrete ChaCha8Rng in sim_cli/sim_daemon.
- **Wear system:** `WearState` (0.0–1.0) on each module. 3-band efficiency: nominal/degraded/critical. Auto-disables at 1.0. Maintenance Bay repairs most-worn, consumes RepairKit.
- **Economy system:** Balance starts at $1B. Import/export in apply_commands. Ship construction requires tech_ship_construction. Pricing from pricing.json.
- **Event sync:** When adding a new `Event` variant to `sim_core/src/types.rs`, you MUST also add a handler in `ui_web/src/hooks/applyEvents.ts` (or add to the allow-list in `scripts/ci_event_sync.sh` if intentionally skipped). CI enforces this.
- **Time scale:** `minutes_per_tick` in constants.json (default 60 = 1 tick per hour). Test fixtures use 1. Helpers: `Constants::game_minutes_to_ticks()`, `Constants::rate_per_minute_to_per_tick()`. `trade_unlock_tick()` derives from this constant.

## Development Workflow

### Project Tracking (Linear)

Issues tracked in Linear (VioletSpaceCadet workspace, MCP integration configured). Create issues for bugs, features, balance recommendations. Organize into projects, set blocking relationships.

### Feature Development (Multi-Ticket Projects)

Use the `/project-implementation <project>` command to run the full workflow end-to-end. It reads Linear tickets, creates branches, implements code, dispatches the pr-reviewer agent, merges ticket PRs, and delivers a final PR for owner approval. See `.claude/commands/project-implementation.md` for the full process.

Manual summary of the branching model:

1. **Create a feature branch** from main: `feat/<project-name>`
2. **Each ticket gets its own branch** off the feature branch: `feat/<project>/<ticket-id>-<short-name>`
3. **PR per ticket into the feature branch** — pr-reviewer agent reviews, Claude auto-merges after CI + clean review (squash merge)
4. **Final PR from feature branch into main** — merge main into feature branch first to resolve conflicts, then requires owner (@VioletSpaceCadet) approval
5. **Clean up** — delete feature branch and sub-branches after merge

### Small Changes (Single-Ticket)

Branch from main (`fix/<ticket-id>-<short-name>` or `chore/<short-name>`), PR directly into main, owner approval required.

### Pull Request Workflow

**Branch protection on `main`:** Direct pushes blocked, required CI checks ("Rust", "Web", "Bench smoke"), CODEOWNERS review required, stale reviews dismissed.

**PR reviews use the `pr-reviewer` agent** (`.claude/agents/pr-reviewer`). Dispatch it via the Task tool after CI passes. It handles the full review: reads the diff, checks for issues, and posts a review comment on the PR.

**Creating a PR:** Push branch, `gh pr create`. Include Summary + Test plan. Use `--base feat/project-name` when targeting a feature branch.

**Two merge paths:**
- **PR into feature branch:** Claude auto-merges after CI green + pr-reviewer clean (`gh pr merge --squash`)
- **PR into main:** pr-reviewer reviews but Claude NEVER merges — owner must approve and merge

**NEVER push directly to main.**

## After Every Change

Tests run automatically via PostToolUse hook (`.claude/hooks/after-edit.sh`) on `.rs` edits — `cargo fmt` then `cargo test -p <crate>`. Fix failures before moving on.

- **If you changed a type or tick ordering:** update this file and `docs/reference.md` as needed.
- **Before claiming work is complete:** confirm tests pass, no TODO stubs introduced.

## Merging

**Always squash merge. Never push directly to main.**

- **Ticket PR into feature branch:** CI pass → pr-reviewer agent → Claude runs `gh pr merge --squash`
- **PR into main:** CI pass → pr-reviewer agent → Owner approves and squash merges

## Simulation Testing & Balance Analysis

Use the **sim-e2e-tester agent** (`.claude/agents/sim-e2e-tester`) for balance analysis, bulk simulation runs, and E2E simulation diagnostics. It has detailed MCP tool docs (parameters, return shapes, sequencing), diagnostic methodology, and testing workflows.

Use the **fe-chrome-tester agent** (`.claude/agents/fe-chrome-tester`) for browser-based UI testing. Requires `--chrome` flag. Tests panel rendering, SSE streaming, speed controls, alerts, economy, and save system at `localhost:5173`.

**E2E tests** (`e2e/`) are intentionally minimal — they cover SSE streaming, pause/resume, speed controls, save, and spacebar toggle. Don't add complex E2E tests; they're fragile and better covered by vitest unit tests or the sim-e2e-tester agent with Chrome.

## Skills

Before starting implementation work, scan `.claude/skills/` for relevant domain skills. Match the task description against skill `triggers` in frontmatter. For each matched skill:
1. Read the skill file
2. Follow its checklist and testing guidance
3. Use any agents listed in its `agents` field

If no skills match, read `.claude/skills/general.md`. After loading, print a brief summary:
> Skills loaded: **frontend-dev** (task touches ui_web), **cross-layer-e2e** (spans FE + daemon)
> Required agents: fe-chrome-tester, sim-e2e-tester

See `.claude/skills/README.md` for how to add or edit skills.

## Notes

- IDE: RustRover (JetBrains)
- Mutation testing with `cargo-mutants`
- **`cargo` is on PATH.** Never prefix with `PATH=`, `export PATH=`, or `~/.cargo/bin/`. Just use `cargo test`, `cargo build`, etc. For worktrees use `--manifest-path`. The PreToolUse hook (`check-bash.sh`) enforces this.
- **For `gh pr` bodies**, use `--body-file /tmp/pr-body.md` instead of inline `--body`. Claude Code blocks `$()`, `${}`, and quoted flag-like strings in inline text.
