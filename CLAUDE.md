# CLAUDE.md

Space industry simulation game. Deterministic Rust sim core, HTTP daemon with SSE event streaming, React mission control UI.

`docs/DESIGN_SPINE.md` — authoritative design philosophy. `docs/reference.md` — detailed types, content files, inventory/refinery design. `base-project.md` — original design doc. Balance analysis and tuning tracked in Linear ("Balance & Tuning" project, VioletSpaceCadet workspace).

## Common Commands

```bash
cargo build                                               # Build all crates
cargo test                                                # Run all tests
cargo test -p sim_core                                    # Test sim_core only
cargo test <name>                                         # Run a single test by name
cargo clippy                                              # Lint
cargo fmt                                                 # Format

# CLI runner
cargo run -p sim_cli -- run --ticks 1000 --seed 42
cargo run -p sim_cli -- run --ticks 500 --seed 42 --print-every 50 --event-level debug
cargo run -p sim_cli -- run --state content/dev_base_state.json
cargo run -p sim_cli -- run --ticks 500 --seed 42 --metrics-every 60
cargo run -p sim_cli -- run --ticks 500 --seed 42 --no-metrics

# HTTP daemon (http://localhost:3001)
cargo run -p sim_daemon -- run --seed 42
cargo run -p sim_daemon -- run --seed 42 --ticks-per-sec 0

# React UI (in ui_web/)
cd ui_web && npm run dev     # dev at http://localhost:5173 (proxies /api to :3001)
cd ui_web && npm test        # vitest

# Benchmark runner
cargo run -p sim_bench -- run --scenario scenarios/baseline.json
cargo run -p sim_bench -- run --scenario scenarios/balance_v1.json
cargo run -p sim_bench -- run --scenario scenarios/cargo_sweep.json --output-dir /tmp/bench
```

## Architecture

Cargo workspace: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`. Plus `sim_world` (shared content loading + world gen) and `ui_web/` (React).

- **sim_core** — Pure deterministic sim. No IO. Modules: `types`, `engine`, `tasks`, `research`, `station`, `graph`, `id`, `composition`, `metrics`, `wear`. Public API: `tick()`, `inventory_volume_m3()`, `mine_duration()`, `shortest_hop_count()`, `generate_uuid()`, `compute_metrics()`, `write_metrics_csv()`, `write_metrics_header()`, `append_metrics_row()`, `wear_efficiency()`.
- **sim_control** — `AutopilotController` (deposit→mine→deepscan→survey priority + station module auto-management). Skips re-enabling modules at max wear. Auto-assigns labs to eligible techs.
- **sim_world** — `load_content()` + `build_initial_state()`. Content from `content/*.json` (8 files incl. `component_defs.json`).
- **sim_bench** — Automated scenario runner. Loads JSON scenario files with optional `"state"` field (e.g., `"./content/dev_base_state.json"`), applies constant overrides AND module-level overrides (dotted keys: `module.lab.research_interval_ticks`, `module.processor.processing_interval_ticks`, etc.). Runs N seeds in parallel (rayon). Writes per-seed `run_result.json` (schema v1) + `metrics_000.csv`, and batch-level `batch_summary.json` with aggregated metrics (mean/min/max/stddev). Collapse detection (refinery starved + fleet idle). Output: `runs/<name>_<timestamp>/`.
- **sim_cli** — CLI tick loop with autopilot. `--state`, `--metrics-every`, `--no-metrics` flags. Auto-writes to `runs/<run_id>/`.
- **sim_daemon** — axum 0.7. SSE (50ms flush, 200ms heartbeat). `--metrics-every` flag (default 60), `--no-metrics`. Auto-writes to `runs/<run_id>/`. AlertEngine evaluates 9 pure-Rust rules after each metrics sample, emits `AlertRaised`/`AlertCleared` events on SSE. `AtomicBool` pause flag checked by tick loop (no Mutex). Endpoints: `/api/v1/meta`, `/api/v1/snapshot`, `/api/v1/metrics`, `/api/v1/stream`, `POST /api/v1/save`, `POST /api/v1/pause`, `POST /api/v1/resume`, `GET /api/v1/alerts` (active alerts).
- **ui_web** — Vite 7 + React 19 + TS 5 + Tailwind v4. `useSimStream` (useReducer + applyEvents), `useAnimatedTick` (60fps interpolation), `useSortableData`. Draggable panels via @dnd-kit (Map, Events, Asteroids, Fleet, Research). Fleet panel has expandable rows with detail sections. StatusBar: alert badges (dismissible, color-coded), pause/resume toggle, save button. Keyboard shortcuts: spacebar (pause/resume), Cmd/Ctrl+S (save). Web Audio sound effects (`sounds.ts`) for pause, resume, and save. `useAnimatedTick` freezes display tick immediately when paused.

**Tick order:** 1. Apply commands → 2. Resolve ship tasks → 3. Tick station modules (3a processors, 3b assemblers, 3c sensor arrays, 3d labs, 3e maintenance) → 4. Advance research (batch roll every N ticks) → 5. Replenish scan sites → 6. Increment tick.

**Key design rules:**
- Asteroids created on discovery (scan_sites → AsteroidState), not pre-populated.
- Research uses lab-based domain system. Labs consume raw data, produce domain-specific points. Tech unlock is probabilistic with domain sufficiency.
- Raw data is sim-wide (on ResearchState), not station inventory.
- Research rolls every N ticks (configurable), not every tick.
- DeepScan commands dropped if no unlocked tech has EnableDeepScan effect.
- All collection iteration sorted by ID before RNG use for determinism.
- Scan sites replenished when count < 5. Deterministic UUIDs from seeded RNG.
- sim_core takes `&mut impl rand::Rng` — concrete ChaCha8Rng in sim_cli/sim_daemon.
- **Wear system:** `WearState` (0.0–1.0) on each module. Processors accumulate `wear_per_run` after each run. 3-band efficiency: nominal (1.0), degraded (0.75 at ≥0.5), critical (0.5 at ≥0.8). Auto-disables at 1.0. Maintenance Bay repairs most-worn module, consumes RepairKit. `WearState` is generic — designed for future ship module wear.

## Development Workflow

### Project Tracking (Linear)

Issues are tracked in Linear (VioletSpaceCadet workspace, MCP integration configured). Use the Linear MCP tools to:
- Create issues for bugs, features, and balance recommendations
- Organize into projects (e.g., "Balance & Tuning") for related work
- Set blocking relationships between dependent issues
- Update issues with sim results and revised proposals

### Balance & Tuning Loop

1. **Run sim_bench scenarios** — `scenarios/baseline.json` (current defaults) or custom scenario with overrides
2. **Analyze results** — inspect `batch_summary.json` aggregated metrics, per-seed `run_result.json`, and `metrics_000.csv` time series
3. **File Linear tickets** — create issues with sim data, proposed changes, and rationale
4. **Test via overrides** — use `module.*` dotted keys in scenario overrides to test changes without editing content files
5. **Apply to content** — once validated, update `content/constants.json` or `content/module_defs.json`
6. **Re-run and verify** — confirm metrics improve, no regressions

### Feature Development

For larger features (new modules, new systems, sim_core changes):

1. **Create Linear issues** — scope the work, set priorities and dependencies
2. **Work in a git worktree** — `git worktree add .worktrees/<name> -b feat/<name>` for isolation
3. **Implement and test** — iterate in the worktree, run `cargo test`, run sim_bench scenarios
4. **Squash merge to main** — `git merge --squash <branch>`, clean commit message
5. **Clean up** — `git worktree remove .worktrees/<name>` + `git branch -D feat/<name>`

### Scenario Files

| Scenario | Ticks | Purpose |
|---|---|---|
| `scenarios/baseline.json` | 20,160 (2 weeks) | Current defaults with `dev_base_state.json` |
| `scenarios/balance_v1.json` | 20,160 (2 weeks) | Module tuning proposals (lab/refinery/assembler overrides) |
| `scenarios/month.json` | 43,200 (30 days) | Medium-term sustainability (refinery throughput, kit economy) |
| `scenarios/quarter.json` | 129,600 (90 days) | Long-term sustainability (slag buildup, research progress) |
| `scenarios/cargo_sweep.json` | 10,000 | Cargo capacity stress test |

Scenarios support: `"state"` (path to initial state JSON), `"overrides"` (constants + `module.*` keys), `"seeds"` (list or `{"range": [1, 5]}`).

### Content & Starting State

- `content/dev_base_state.json` — canonical starting state for gameplay testing (refinery, assembler, maintenance bay, 2 labs, 500 kg Fe, 10 repair kits, 50 m³ ship cargo, 2,000 m³ station cargo)
- `content/constants.json` — game constants (already rebalanced for hard sci-fi pacing)
- `content/module_defs.json` — module behavior parameters (intervals, wear, recipes)
- `build_initial_state()` in sim_world should stay in sync with `dev_base_state.json`

## After Every Change

Tests run automatically via PostToolUse hook (`.claude/hooks/after-edit.sh`) on `.rs` edits — `cargo fmt` then `cargo test -p <crate>` for the edited crate. Fix failures before moving on.

- **If you changed a type or tick ordering:** update this file and `docs/reference.md` as needed.
- **Before claiming work is complete:** confirm tests pass, no TODO stubs introduced.

## Merging Branches to Main

**Always squash merge.** Never fast-forward or merge commits.

```bash
git merge --squash <branch-name>
git commit  # subject: feat(scope): summary, body: bullet list, Co-Authored-By trailer
```

After merging: `git worktree remove .worktrees/<name>` + `git branch -D <branch-name>`

## Notes

- IDE: RustRover (JetBrains)
- Mutation testing with `cargo-mutants`
