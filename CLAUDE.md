# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Space industry simulation game. Deterministic Rust sim core, HTTP daemon with SSE event streaming, React mission control UI.
See `base-project.md` for the original design doc and `mvp0-contract.md` for the authoritative MVP-0 type/mechanic spec.

## Common Commands

```bash
cargo build                                               # Build all crates
cargo test                                                # Run all tests
cargo test -p sim_core                                    # Test sim_core only
cargo test <name>                                         # Run a single test by name
cargo clippy                                              # Lint
cargo fmt                                                 # Format
cargo mutants                                             # Mutation testing (requires cargo-mutants)

# CLI runner
cargo run -p sim_cli -- run --ticks 1000 --seed 42
cargo run -p sim_cli -- run --ticks 500 --seed 42 --print-every 50 --event-level debug

# HTTP daemon (runs at http://localhost:3001)
cargo run -p sim_daemon -- run --seed 42
cargo run -p sim_daemon -- run --seed 42 --ticks-per-sec 0   # as fast as possible
cargo run -p sim_daemon -- run --seed 42 --port 3001 --ticks-per-sec 10 --max-ticks 500

# React UI (in ui_web/)
cd ui_web && npm run dev          # dev server at http://localhost:5173 (proxies /api to :3001)
cd ui_web && npm test             # run vitest tests
cd ui_web && npm run build        # production build
```

## Architecture

Cargo workspace with four crates + a React app. Dependency order: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`.

### `crates/sim_core` (lib)
Pure deterministic simulation. No IO, no network.

**Public API:** a single `tick(state, commands, content, rng, event_level) -> Vec<EventEnvelope>` function.

**Tick order (per call):**
1. Apply commands scheduled for this tick (ownership-checked).
2. Resolve ship tasks whose `eta_tick` has arrived.
3. Advance station research on all eligible techs.
4. Increment `state.meta.tick`.

**Key design rules:**
- Asteroids do not exist in state until surveyed. `state.scan_sites` holds pre-generated sites; survey completion creates and inserts an `AsteroidState`.
- Research is fully automatic — compute distributes evenly across all eligible techs (prereqs met, not yet unlocked). No player allocation.
- DeepScan commands are silently dropped if no unlocked tech has the `EnableDeepScan` effect.
- All collection iteration is sorted by ID before RNG consumption to guarantee determinism.

### `crates/sim_control` (lib)
Command sources. Implements the `CommandSource` trait.

- `AutopilotController`: four-priority loop per idle ship:
  1. Deposit cargo at nearest station (if hold non-empty)
  2. Mine best deep-scanned asteroid (sorted by `mass_kg × Fe_fraction` desc)
  3. Deep-scan IronRich asteroids (if `tech_deep_scan_v1` unlocked)
  4. Survey unscanned sites
- `ScenarioSource`: stub — will replay scripted tick→commands from JSON.

### `crates/sim_cli` (bin)
Loads content, builds initial world state, runs the tick loop with autopilot.

- Content loaded from `content/*.json` (4 files assembled into `GameContent`).
- World gen creates 1 station + 1 ship + N scan sites (from templates × `asteroid_count_per_template`).
- Prints a status line every `--print-every` ticks; always prints on tech unlock.

### `crates/sim_daemon` (bin)
axum 0.7 HTTP server that runs the tick loop in a tokio task and streams events to the React UI.

- Shared state: `Arc<Mutex<SimState>>` (game state + RNG + autopilot).
- Events broadcast via `tokio::sync::broadcast::Sender<Vec<EventEnvelope>>`.
- Tick rate configurable (`--ticks-per-sec`; 0 = as fast as possible).
- **Endpoints:** `GET /api/v1/meta`, `GET /api/v1/snapshot`, `GET /api/v1/stream` (SSE).
- CORS configured for `http://localhost:5173` (Vite dev server).

### `ui_web/` (React app — not a Rust crate)
Vite 5 + React 18 + TypeScript 5 mission control dashboard. Tests via Vitest + React Testing Library.

- **Data flow:** on mount fetches `/api/v1/snapshot` to hydrate, then subscribes to `/api/v1/stream` (SSE) for live updates.
- **State:** single `useReducer` in `useSimStream` hook; no external state library.
- **Layout:** StatusBar (tick/time/connection) + four resizable panels (react-resizable-panels v2): EventsFeed | AsteroidTable | FleetPanel | ResearchPanel.
- **Proxy:** `/api` proxied to `http://localhost:3001` in dev (`vite.config.ts`).

## Key Types (sim_core)

| Type | Purpose |
|---|---|
| `GameState` | Full mutable simulation state |
| `ScanSite` | Unscanned potential asteroid location (consumed on survey) |
| `AsteroidState` | Created on discovery; holds `true_composition` (hidden), `knowledge`, and `mass_kg` |
| `ResearchState` | `unlocked`, `data_pool`, `evidence` — no active allocations |
| `TaskKind` | `Idle`, `Survey`, `DeepScan`, `Mine { asteroid, duration_ticks }`, `Deposit { station }`, `Transit { destination, total_ticks, then }` |
| `Command` | `AssignShipTask { ship_id, task_kind }` |
| `GameContent` | Static config: techs, solar system, asteroid templates, elements, constants |
| `ElementDef` | `id`, `density_kg_per_m3`, `display_name` — used for cargo volume calculations |
| `TechEffect` | `EnableDeepScan` or `DeepScanCompositionNoise { sigma }` |

**Cargo model:** ships and stations both carry `cargo: HashMap<ElementId, f32>` (kg). Volume constraint: `Σ(cargo[el] / density[el]) ≤ capacity_m3`. Mining produces ore keyed as `"ore:{asteroid_id}"` — see Ore Design below.

## Content Files

All in `content/`. Loaded at runtime by `sim_cli` and `sim_daemon`; never compiled in.

| File | Key fields |
|---|---|
| `constants.json` | Scan durations, travel ticks, mining rate, cargo capacities, deposit ticks |
| `techs.json` | Tech tree (`tech_deep_scan_v1` is the only current tech) |
| `solar_system.json` | Nodes and edges (static graph) |
| `asteroid_templates.json` | Composition ranges and anomaly tags per template |
| `elements.json` | Element definitions: `id`, `density_kg_per_m3`, `display_name`. Includes special `ore` entry (3000 kg/m³) used as density fallback for all `ore:*` cargo keys |

## After Every Change

Tests run automatically via a PostToolUse hook (`.claude/hooks/after-edit.sh`) whenever a `.rs` file is edited — `cargo fmt` then `cargo test`. Fix any failures before moving on.

Additionally, use judgment on the following:

- **If you changed a type, added/removed a crate, or changed tick ordering:** update the relevant section in CLAUDE.md and `memory/MEMORY.md`.
- **If you added a new mechanic or system:** add it to the Key Types table and Architecture section in CLAUDE.md.
- **Before claiming work is complete:** confirm tests pass and no `TODO` stubs were introduced without being noted.

## Ore Design

Mining extracts **raw ore**, not pre-split elements. This reflects the real-world reality that ore is a mixed material requiring separate refining.

**Current model (`ore:{asteroid_id}` keys):**
- Each asteroid produces a distinct cargo item `"ore:asteroid_0000"`, `"ore:asteroid_0001"`, etc.
- Different ores never blend — they accumulate separately in ship holds and station storage.
- `cargo_volume_used` handles the `ore:` prefix by looking up the generic `ore` element density (3000 kg/m³).
- `OreMined` event carries `ship_id` and `asteroid_id`; FE caches the asteroid's known composition to display alongside ore kg.

**Future direction (not yet built):**
- **Ore keyed by composition hash**, not asteroid ID. Two asteroids with Fe 71%/Si 29% produce the same ore lot and can blend freely. Key format: `ore:Fe70:Si30` (composition rounded to nearest N%).
- **Blending tolerance** as a tech unlock: basic = ±2%, advanced = ±10%.
- **Volatiles flag ore as unblendable** even if rock composition would otherwise match (He, etc. require separate cryo handling).
- **Slag** is a single blended bulk commodity `slag: X kg` — composition is not tracked. This avoids inventory explosion while keeping disposal mechanics meaningful (hold limits, venting penalties, further refining).

## MVP Scope

- **MVP-0 (done):** `sim_core` tick + tests, `sim_control` autopilot, `sim_cli` run loop.
- **MVP-1 (done):** `sim_daemon` HTTP server (axum + tokio), SSE event stream, React mission control UI.
- **MVP-2 (done):** Mining loop — cargo holds, Mine/Deposit tasks, ore extraction, autopilot priority reorder, FE fleet/station cargo display with per-lot ore composition.

## Merging Branches to Main

**Always squash merge** when merging feature branches or worktrees into main. Never fast-forward or create merge commits.

```bash
# From the main branch:
git merge --squash <branch-name>
git commit  # with AI-generated message (see below)
```

**Commit message format** — read `git log main..<branch>` to understand all the changes, then write a single squash commit message:
- Subject line: `feat(scope): short summary` (or `fix`, `refactor`, etc.)
- Body: bullet list of the key changes from the branch (not a 1:1 copy of each commit — summarize and group logically)
- End with `Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>`

**After merging**, clean up the branch and worktree:
```bash
git worktree remove .worktrees/<name>   # if it was a worktree
git branch -D <branch-name>             # delete the local branch
```

## Notes

- IDE: RustRover (JetBrains)
- RNG: `ChaCha8Rng` (portable, seeded) in `sim_cli`; `sim_core` takes `&mut impl rand::Rng` to stay IO-free
- Mutation testing with `cargo-mutants` is part of the workflow
