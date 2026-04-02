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
cargo run -p sim_cli -- run --state content/dev_advanced_state.json
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

pip install duckdb pyarrow ruff pytest pytest-cov mypy    # Install Python deps (once, in venv)
ruff check scripts/analysis/                              # Python lint
ruff format scripts/analysis/                             # Python format
mypy scripts/analysis/                                    # Python type check
pytest scripts/analysis/tests/                            # Python tests

./scripts/ci_rust.sh                                      # fmt + clippy + test + coverage
./scripts/ci_web.sh                                       # npm ci + lint + tsc + vitest + coverage
./scripts/ci_python.sh                                    # ruff + mypy + pytest + coverage
./scripts/ci_bench_smoke.sh                               # Release build + ci_smoke scenario
./scripts/ci_e2e.sh                                       # E2E Playwright tests
./scripts/ci_event_sync.sh                                # Event exhaustiveness check

CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release -p sim_cli  # Build with debug symbols
samply record target/release/sim_cli run --ticks 10000 --seed 42   # CPU profile (opens browser)
```

## Architecture

Cargo workspace: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`. Plus `sim_world` (content loading + world gen) and `ui_web/` (React).

- **sim_core** — Pure deterministic sim. No IO. Public API: `tick()`, `inventory_volume_m3()`, `mine_duration()`, `TickTimings`, `compute_step_stats()`, etc.
- **sim_control** — `AutopilotController` with hierarchical agents: `StationAgent` (per-station modules, labs, crew, trade, ship objectives) and `ShipAgent` (per-ship tactical execution: transit, mine, deposit, refuel).
- **sim_world** — `load_content()` + `build_initial_state()`. Content from `content/*.json`.
- **sim_bench** — Scenario runner. JSON overrides (constants + `module.*` dotted keys). Parallel seeds via rayon. Outputs Parquet + CSV metrics.
- **scripts/analysis** — Python ML data pipeline (DuckDB + pyarrow). Feature extraction, outcome labeling, cross-seed analysis. Tooling: ruff (lint/format), mypy (types), pytest (tests).
- **sim_cli** — CLI tick loop with autopilot. `--state`, `--metrics-every`, `--no-metrics` flags.
- **sim_daemon** — axum 0.7, SSE, AlertEngine, pause/resume, command queue. See `docs/reference.md` for endpoints. Includes `analytics` module (trend/rate/bottleneck analysis) and `GET /api/v1/advisor/digest` endpoint.
- **mcp_advisor** — MCP server (TypeScript, stdio transport) for balance analysis and knowledge capture. Tools: metrics digest, alerts, game parameters, parameter proposals, sim lifecycle, `save_run_journal`, `query_knowledge`, `update_playbook`. Auto-discovered via `.mcp.json`. Requires running `sim_daemon` for sim tools; knowledge tools work standalone.
- **e2e** — Playwright E2E smoke tests. Global setup spawns daemon (port 3002) + Vite (port 5174). Kept minimal for CI stability; use Chrome browser tools for ad-hoc UI testing.
- **ui_web** — Vite 7 + React 19 + TS 5 + Tailwind v4. Draggable panels, SSE streaming, keyboard shortcuts.

**Tick order:** 1. Apply commands → 2. Resolve ship tasks → 3. Tick station modules (processors, assemblers, sensors, labs, maintenance, 3.6 thermal, 3.7 boiloff) → 4. Advance research → 5. Replenish scan sites → 6. Increment tick.

**Key design rules:**
- Asteroids created on discovery (scan_sites → AsteroidState), not pre-populated.
- Research uses lab-based domain system. Labs consume raw data, produce domain-specific points. Tech unlocks deterministically when all domain requirements are met.
- Raw data is sim-wide (on ResearchState), not station inventory.
- DeepScan commands dropped if no unlocked tech has EnableDeepScan effect.
- All collection iteration sorted by ID before RNG use for determinism.
- sim_core takes `&mut impl rand::Rng` — concrete ChaCha8Rng in sim_cli/sim_daemon.
- **Wear system:** `WearState` (0.0–1.0) on each module. 3-band efficiency: nominal/degraded/critical. Auto-disables at 1.0. Maintenance Bay repairs most-worn, consumes RepairKit.
- **Economy system:** Balance starts at $1B. Import/export in apply_commands. Ship construction requires tech_ship_construction. Pricing from pricing.json.
- **Thermal system:** Modules with `ThermalDef` track temperature in milli-Kelvin (`ThermalState`). Modules initialize at ambient temp (293K). `ThermalDef` supports optional `idle_heat_generation_w` for continuous preheating when enabled. Smelter (Processor with thermal req) generates heat per run, stalls if too cold, yield/quality scale with temp. Radiator provides `cooling_capacity_w` shared across thermal group. Tick step 3.6 has 3 passes: idle heat generation → passive cooling (Newton's law) → radiator cooling. Overheat zones: Nominal/Warning (2x wear)/Critical (4x wear, auto-disable).
- **Event sync:** When adding a new `Event` variant to `sim_core/src/types.rs`, you MUST also add a handler in `ui_web/src/hooks/applyEvents.ts` (or add to the allow-list in `scripts/ci_event_sync.sh` if intentionally skipped). CI enforces this.
- **Time scale:** `minutes_per_tick` in constants.json (default 60 = 1 tick per hour). Test fixtures use 1. Helpers: `Constants::game_minutes_to_ticks()`, `Constants::rate_per_minute_to_per_tick()`. `trade_unlock_tick()` derives from this constant.
- **Content-driven types:** `AnomalyTag`, `DataKind`, `ResearchDomain` are loaded from content JSON. Adding a new type = adding a JSON entry, not a Rust enum variant. Enums are reserved for engine mechanics (Command, Event, TaskKind), not content categories.
- **Instrumentation:** `TickTimings` struct (14 `Duration` fields: 6 top-level tick steps + 8 station sub-steps). `timed!` macro wraps each step — active in debug builds via `debug_assertions`, compiled away in release unless `instrumentation` feature enabled. `tick()` takes `Option<&mut TickTimings>` — pass `None` for zero-cost, `Some(&mut timings)` to collect. `compute_step_stats(&[TickTimings])` returns per-step mean/p50/p95/max. sim_bench and sim_daemon both enable the feature and collect timings. Daemon exposes `GET /api/v1/perf` (rolling 1,000-tick buffer) and includes perf summary in advisor digest.

## Development Workflow

### Project Tracking (Linear)

Issues tracked in Linear (VioletSpaceCadet workspace, MCP integration configured). Create issues for bugs, features, balance recommendations. Organize into projects, set blocking relationships.

### Default: Single-Ticket Workflow (`/implement`)

Use `/implement <ticket-id>` for most work. Autonomous end-to-end: read ticket → branch from main → implement → PR → CI → pr-reviewer → fix → compound (if non-trivial) → merge into main.

Claude **auto-merges into main** after CI green + pr-reviewer clean (no unresolved should-fix items). No owner approval needed for ticket PRs that pass review.

See `.claude/commands/implement.md` for the full process.

### Batch: Multi-Ticket Workflow (`/project-implementation`)

Use `/project-implementation <project>` or `/project-implementation VIO-1 VIO-2 VIO-3` to queue multiple tickets. Same direct-to-main flow as `/implement`, but loops through all tickets in dependency order with `/compact` between each to keep context fresh.

See `.claude/commands/project-implementation.md` for the full process.

### Pull Request Workflow

**Branch protection on `main`:** Direct pushes blocked, required CI checks ("Rust", "Web", "Bench smoke"), CODEOWNERS review required, stale reviews dismissed.

**PR reviews use the `pr-reviewer` agent** (`.claude/agents/pr-reviewer`). Dispatch it via the Agent tool after CI passes. It handles the full review: reads the diff, checks for issues, and posts a review comment on the PR.

**Creating a PR:** Push branch, `gh pr create`. Include Summary + Test plan. Use `--base feat/project-name` when targeting a feature branch.

**Merge policy:**
- **Ticket PR into main (via `/implement`):** Claude auto-merges after CI green + pr-reviewer clean (`gh pr merge --squash --delete-branch`)
- **Ticket PR into feature branch:** Claude auto-merges after CI green + pr-reviewer clean
- **Feature branch PR into main:** Owner must approve and merge

**NEVER push directly to main.**

### Knowledge Capture (`ce:compound`)

After implementing a ticket that involved debugging, new patterns, or tricky solutions, run `ce:compound` to document the learning in `docs/solutions/`. Skip for routine/simple changes. The `/implement` command does this automatically.

## After Every Change

Tests run automatically via PostToolUse hook (`.claude/hooks/after-edit.sh`) on `.rs` edits — `cargo fmt` then `cargo test -p <crate>`. Fix failures before moving on.

- **If you changed a type or tick ordering:** update this file and `docs/reference.md` as needed.
- **Before claiming work is complete:** confirm tests pass, no TODO stubs introduced.

## Merging

**Always squash merge. Never push directly to main.**

- **Ticket PR into main (via `/implement`):** CI pass → pr-reviewer clean → Claude runs `gh pr merge --squash --delete-branch`
- **Ticket PR into feature branch:** CI pass → pr-reviewer clean → Claude runs `gh pr merge --squash`
- **Feature branch PR into main:** CI pass → pr-reviewer reviews → Owner approves and squash merges

## Simulation Testing & Balance Analysis

Use the **sim-e2e-tester agent** (`.claude/agents/sim-e2e-tester`) for balance analysis, bulk simulation runs, and E2E simulation diagnostics. It has detailed MCP tool docs (parameters, return shapes, sequencing), diagnostic methodology, and testing workflows.

Use the **fe-chrome-tester agent** (`.claude/agents/fe-chrome-tester`) for browser-based UI testing. Requires `--chrome` flag. Tests panel rendering, SSE streaming, speed controls, alerts, economy, and save system at `localhost:5173`.

Use the **perf-reviewer agent** (`.claude/agents/perf-reviewer`) for CPU profiling and performance regression detection. Requires `--chrome` flag for flamegraph analysis. Runs samply profiles (500k+ ticks), reads sim_bench timing stats, browses Firefox Profiler Call Trees, and compares before/after results. Use after optimization PRs or when tick logic changes.

**E2E tests** (`e2e/`) are intentionally minimal — they cover SSE streaming, pause/resume, speed controls, save, and spacebar toggle. Don't add complex E2E tests; they're fragile and better covered by vitest unit tests or the sim-e2e-tester agent with Chrome.

## Knowledge System

Game knowledge is captured in `content/knowledge/` and accessed via MCP tools on the balance-advisor server.

**Files:**
- `content/knowledge/journals/*.json` — Run journal entries (per-session observations, bottlenecks, strategy notes)
- `content/knowledge/playbook.md` — Living strategy document (bottleneck resolutions, parameter relationships, fleet sizing)
- `docs/run-journal-schema.md` — Journal schema reference
- `mcp_advisor/src/types.ts` — TypeScript types for journal entries

**MCP Tools (balance-advisor):**
- `query_knowledge` — Search journals + playbook by text, tags, or source. **Use before starting analysis** to check past observations.
- `save_run_journal` — Persist analysis session findings. Auto-generates UUID + timestamp, writes to `content/knowledge/journals/`.
- `update_playbook` — Append to or replace sections of the strategy playbook.

**Workflow — follow this loop during balance analysis sessions:**
1. **Recall:** Call `query_knowledge` with relevant tags/keywords to check what's already known
2. **Analyze:** Run simulation, observe metrics, identify patterns
3. **Record:** Call `save_run_journal` with observations, bottlenecks, alerts, and strategy notes
4. **Generalize:** When a pattern is confirmed across multiple runs, call `update_playbook` to add it as a strategy entry

## Skills

Before starting implementation work, scan `.claude/skills/` for relevant domain skills. Match the task description against skill `triggers` in frontmatter. For each matched skill:
1. Read the skill file
2. Follow its checklist and testing guidance
3. Use any agents listed in its `agents` field

If no skills match, read `.claude/skills/general.md`. After loading, print a brief summary:
> Skills loaded: **frontend-dev** (task touches ui_web), **cross-layer-e2e** (spans FE + daemon)
> Required agents: fe-chrome-tester, sim-e2e-tester

See `.claude/skills/README.md` for how to add or edit skills.

## Rust Analyzer MCP (LSP Tools)

The `rust-analyzer` MCP plugin provides IDE-quality Rust intelligence. **Prefer these over grep/read for Rust navigation tasks** — they understand Rust semantics (traits, impls, re-exports, generics) where text search cannot.

**Setup:** In worktrees, call `rust_analyzer_set_workspace` with the worktree root at session start.

**High-value tools (use proactively):**
- `rust_analyzer_definition` — Jump to where a type/function/trait is defined. **Use instead of grepping for `struct Foo` or `fn bar`.** Returns exact file + line. Params: `file_path`, `line` (0-based), `character` (0-based).
- `rust_analyzer_references` — Find all usages of a symbol across the entire workspace. **Use instead of `Grep` when you need all callers of a function, all uses of a type, or all implementors.** Understands re-exports and trait impls. Can produce large output for common types.
- `rust_analyzer_hover` — Get the full type signature of any symbol at a position. Great for understanding function params/returns without reading surrounding code.
- `rust_analyzer_symbols` — List all symbols (functions, structs, consts, modules) in a file with line ranges. **Use instead of `Read` when you just need a file's structure overview.**
- `rust_analyzer_diagnostics` — Get rust-analyzer's **native** diagnostics for a single file (type mismatches, unresolved imports, syntax errors). **Does NOT include cargo/rustc/clippy diagnostics** — those only come from `cargo check`. Use for quick "does this type-check?" feedback, but always run `cargo check`/`cargo test` for full validation.

**Situational tools:**
- `rust_analyzer_completion` — Explore what methods/fields are available on a type at a position. Output is very large (~250K chars, no server-side filtering) — use sparingly.
- `rust_analyzer_code_actions` — See available refactorings (extract function, destructure, etc.) at a range. List-only — cannot resolve or apply edits.
- `rust_analyzer_workspace_diagnostics` — **Unreliable.** rust-analyzer sets `workspace_diagnostics: false` in capabilities. The MCP tool synthesizes results but often returns empty/unexpected format. **Use `cargo check` instead.**
- `rust_analyzer_format` — Format a file. Redundant with the after-edit hook's `cargo fmt`.

**When to use RA vs Grep/Read:**
| Task | Use RA | Use Grep/Read |
|------|--------|---------------|
| Find where a type is defined | `definition` | — |
| Find all callers of a function | `references` | — |
| Check a function's signature | `hover` | — |
| Get a file's symbol outline | `symbols` | — |
| Search for a string literal | — | `Grep` |
| Find files by name pattern | — | `Glob` |
| Read actual code content | — | `Read` |
| Search across non-Rust files | — | `Grep` |

## Notes

- IDE: RustRover (JetBrains)
- Mutation testing with `cargo-mutants`
- **`cargo` is on PATH.** Never prefix with `PATH=`, `export PATH=`, or `~/.cargo/bin/`. Just use `cargo test`, `cargo build`, etc. For worktrees use `--manifest-path`. The PreToolUse hook (`check-bash.sh`) enforces this.
- **For `gh pr` bodies**, use `--body-file /tmp/pr-body.md` instead of inline `--body`. Claude Code blocks `$()`, `${}`, and quoted flag-like strings in inline text.
- **For multi-line Python**, write to `/tmp/script.py` with the Write tool, then run `python3 /tmp/script.py`. Never use `python3 -c` with multi-line strings — Claude Code blocks commands with quoted newlines followed by `#` comments.
- **Never use `cat` heredoc/redirect** to create files. Use the Write tool instead. `cat > file << 'EOF'` triggers built-in safety checks when content contains `#` comments or backticks.
- **For multi-line git commits**, write the message to `/tmp/commit-msg.txt` with the Write tool, then run `git commit -F /tmp/commit-msg.txt`. Never use `git commit -m "$(cat <<'EOF' ...)"` — Claude Code blocks `$()` as a shell operator.
