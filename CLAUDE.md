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

./scripts/ci_rust.sh                                      # fmt + clippy + test
./scripts/ci_web.sh                                       # npm ci + lint + tsc + vitest
./scripts/ci_bench_smoke.sh                               # Release build + ci_smoke scenario
./scripts/ci_e2e.sh                                       # E2E Playwright tests
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

## Development Workflow

### Project Tracking (Linear)

Issues tracked in Linear (VioletSpaceCadet workspace, MCP integration configured). Create issues for bugs, features, balance recommendations. Organize into projects, set blocking relationships.

### Feature Development (Multi-Ticket Projects)

1. **Create a feature branch** from main: `feat/<project-name>`
2. **Each ticket gets its own branch** off the feature branch: `feat/<project>/<ticket-id>-<short-name>`
3. **PR per ticket into the feature branch** — standard Claude review process applies
4. **Claude auto-merges ticket PRs** after CI passes and review is clean (squash merge)
5. **Final PR from feature branch into main** — merge main into feature branch first to resolve conflicts, then requires owner (@VioletSpaceCadet) approval
6. **Clean up** — delete feature branch and sub-branches after merge

### Small Changes (Single-Ticket)

Branch from main (`fix/<ticket-id>-<short-name>` or `chore/<short-name>`), PR directly into main, owner approval required.

### Pull Request Workflow

**Branch protection on `main`:** Direct pushes blocked, required CI checks ("Rust", "Web", "Bench smoke"), CODEOWNERS review required, stale reviews dismissed.

**Mandatory Claude Code PR review:**
1. Watch CI: `gh pr checks N --watch`
2. If fails: `gh run view RUN_ID --log-failed`, fix, push, watch again
3. Once green: fresh review via `gh pr diff N`
4. Post review: `gh pr review N --comment` — must start with "Claude Code Review -- No issues found." or "Claude Code Review -- Issues found:"
5. Do NOT use backticks in review comment bodies (causes permission prompts)

**Creating a PR:** Push branch, `gh pr create`. Include Summary + Test plan. Use `--base feat/project-name` when targeting a feature branch.

**Two merge paths:**
- **PR into feature branch:** Claude auto-merges after CI green + review (`gh pr merge --squash`)
- **PR into main:** Claude reviews but NEVER merges — owner must approve and merge

**NEVER push directly to main.**

## After Every Change

Tests run automatically via PostToolUse hook (`.claude/hooks/after-edit.sh`) on `.rs` edits — `cargo fmt` then `cargo test -p <crate>`. Fix failures before moving on.

- **If you changed a type or tick ordering:** update this file and `docs/reference.md` as needed.
- **Before claiming work is complete:** confirm tests pass, no TODO stubs introduced.

## Merging

**Always squash merge. Never push directly to main.**

- **Ticket PR into feature branch:** CI pass → Claude review → Claude runs `gh pr merge --squash`
- **PR into main:** CI pass → Claude review → Owner approves and squash merges

## Balance Advisor (MCP)

9 MCP tools are available for balance analysis. Use them when investigating simulation balance, tuning parameters, or diagnosing issues:

- **start_simulation** — Start a sim daemon as a background process. Accepts optional `seed` and `max_ticks`. Stops any previous daemon first. Auto-kills on session end.
- **stop_simulation** — Stop a previously started sim daemon.
- **set_speed** — Set simulation tick speed. Default is 10 tps; use 1000+ for fast analysis runs.
- **pause_simulation** / **resume_simulation** — Pause and resume the simulation.
- **get_metrics_digest** — Fetch trend analysis, production rates, and bottleneck detection from the running sim. Use this first when asked about simulation performance or balance problems.
- **get_active_alerts** — Fetch currently firing alerts (e.g. inventory full, starvation, wear critical). Use when diagnosing operational issues.
- **get_game_parameters** — Read content files (constants, module_defs, techs, pricing) without manual file reads. Use when comparing current values to proposed changes.
- **suggest_parameter_change** — Save a proposed balance change with rationale and expected impact to `content/advisor_proposals/`. Use after analyzing metrics to recommend a tuning adjustment.

**Workflow:** Use `start_simulation` to launch a daemon, then `set_speed` to 1000+ tps for fast analysis. Wait for data to accumulate (tens of thousands of ticks), then use `get_metrics_digest` to analyze trends. If something looks off, check `get_active_alerts` and `get_game_parameters` to understand why, then `suggest_parameter_change` to propose a fix. Use `stop_simulation` when done.

**Tips:** Rates may show 0.0 during ship transit periods (2,880 ticks per hop) — this is normal, check again after the ship delivers ore. Trends need 50+ metric samples (captured every 60 ticks) to differentiate short vs long windows.

## Ad-Hoc UI Testing (Chrome Browser)

For interactive UI testing during development, use Chrome browser tools (via Claude Code's `--browser` flag or Claude in Chrome MCP) instead of writing new Playwright tests. This is faster and more flexible for one-off inspection.

**Setup:** Start the daemon and Vite dev server, then navigate to `http://localhost:5173`.

```bash
cargo run -p sim_daemon -- run --seed 42                  # Terminal 1: daemon (:3001)
cd ui_web && npm run dev                                  # Terminal 2: Vite (:5173)
```

**What you can do:** Take screenshots, click buttons (pause/resume, speed presets, save), use keyboard shortcuts (Space, 1-5, Ctrl+S), read panel data, interact with forms. Good for verifying UI state during balance analysis or after FE changes.

**E2E tests** (`e2e/`) are intentionally minimal — they cover SSE streaming, pause/resume, speed controls, save, and spacebar toggle. Don't add complex E2E tests for form interactions; they're fragile and better covered by vitest unit tests or ad-hoc Chrome inspection.

## Notes

- IDE: RustRover (JetBrains)
- Mutation testing with `cargo-mutants`
