# CLAUDE.md

Space industry simulation game. Deterministic Rust sim core, HTTP daemon with SSE event streaming, React mission control UI.

`docs/DESIGN_SPINE.md` — authoritative design philosophy. `docs/reference.md` — detailed types, content files, inventory/refinery design. `base-project.md` — original design doc.

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
```

## Architecture

Cargo workspace: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`. Plus `sim_world` (shared content loading + world gen) and `ui_web/` (React).

- **sim_core** — Pure deterministic sim. No IO. Modules: `types`, `engine`, `tasks`, `research`, `station`, `graph`, `id`, `composition`, `metrics`, `wear`. Public API: `tick()`, `inventory_volume_m3()`, `mine_duration()`, `shortest_hop_count()`, `generate_uuid()`, `compute_metrics()`, `write_metrics_csv()`, `write_metrics_header()`, `append_metrics_row()`, `wear_efficiency()`.
- **sim_control** — `AutopilotController` (deposit→mine→deepscan→survey priority + station module auto-management). Skips re-enabling modules at max wear.
- **sim_world** — `load_content()` + `build_initial_state()`. Content from `content/*.json` (8 files incl. `component_defs.json`).
- **sim_cli** — CLI tick loop with autopilot. `--state`, `--metrics-every`, `--no-metrics` flags. Auto-writes to `runs/<run_id>/`.
- **sim_daemon** — axum 0.7. SSE (50ms flush, 200ms heartbeat). `--metrics-every` flag (default 60), `--no-metrics`. Auto-writes to `runs/<run_id>/`. AlertEngine evaluates 9 pure-Rust rules after each metrics sample, emits `AlertRaised`/`AlertCleared` events on SSE. `AtomicBool` pause flag checked by tick loop (no Mutex). Endpoints: `/api/v1/meta`, `/api/v1/snapshot`, `/api/v1/metrics`, `/api/v1/stream`, `POST /api/v1/save`, `POST /api/v1/pause`, `POST /api/v1/resume`, `GET /api/v1/alerts` (active alerts).
- **ui_web** — Vite 7 + React 19 + TS 5 + Tailwind v4. `useSimStream` (useReducer + applyEvents), `useAnimatedTick` (60fps interpolation), `useSortableData`. Draggable panels via @dnd-kit (Map, Events, Asteroids, Fleet, Research). Fleet panel has expandable rows with detail sections. StatusBar: alert badges (dismissible, color-coded), pause/resume toggle, save button. Keyboard shortcuts: spacebar (pause/resume), Cmd/Ctrl+S (save).

**Tick order:** 1. Apply commands → 2. Resolve ship tasks → 3. Tick station modules (processors, assemblers, then maintenance) → 4. Advance research → 5. Replenish scan sites → 6. Increment tick.

**Key design rules:**
- Asteroids created on discovery (scan_sites → AsteroidState), not pre-populated.
- Research fully automatic — compute splits across eligible techs. No player allocation.
- DeepScan commands dropped if no unlocked tech has EnableDeepScan effect.
- All collection iteration sorted by ID before RNG use for determinism.
- Scan sites replenished when count < 5. Deterministic UUIDs from seeded RNG.
- sim_core takes `&mut impl rand::Rng` — concrete ChaCha8Rng in sim_cli/sim_daemon.
- **Wear system:** `WearState` (0.0–1.0) on each module. Processors accumulate `wear_per_run` after each run. 3-band efficiency: nominal (1.0), degraded (0.75 at ≥0.5), critical (0.5 at ≥0.8). Auto-disables at 1.0. Maintenance Bay repairs most-worn module, consumes RepairKit. `WearState` is generic — designed for future ship module wear.

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
