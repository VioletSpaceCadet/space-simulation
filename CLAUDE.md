# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Space industry simulation game. Deterministic Rust sim core, HTTP daemon with SSE event streaming, React mission control UI.

**`docs/DESIGN_SPINE.md` is the authoritative design philosophy document.** All new features and systems must align with it. Key principles: deterministic core, compounding entropy (not sudden failure), recoverable pressure, automation encouraged but fragile at scale, no heavy physics, metrics-first, complexity only when it creates strategic tradeoffs.

See also `base-project.md` for the original design doc and `mvp0-contract.md` for the MVP-0 type/mechanic spec (foundational types only — see below for current state).

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
cargo run -p sim_cli -- run --state content/dev_base_state.json  # load pre-baked state

# HTTP daemon (runs at http://localhost:3001)
cargo run -p sim_daemon -- run --seed 42
cargo run -p sim_daemon -- run --seed 42 --ticks-per-sec 0   # as fast as possible
cargo run -p sim_daemon -- run --seed 42 --port 3001 --ticks-per-sec 10 --max-ticks 500
cargo run -p sim_daemon -- run --state content/dev_base_state.json  # load pre-baked state

# React UI (in ui_web/)
cd ui_web && npm run dev          # dev server at http://localhost:5173 (proxies /api to :3001)
cd ui_web && npm test             # run vitest tests
cd ui_web && npm run build        # production build
```

## Architecture

Cargo workspace with four crates + a React app. Dependency order: `sim_core` ← `sim_control` ← `sim_cli` / `sim_daemon`.

### `crates/sim_core` (lib)
Pure deterministic simulation. No IO, no network. Modules: `types`, `engine`, `tasks`, `research`, `station`, `graph`.

**Public API:** a single `tick(state, commands, content, rng, event_level) -> Vec<EventEnvelope>` function, plus helpers `inventory_volume_m3()`, `mine_duration()`, `shortest_hop_count()`.

**Tick order (per call):**
1. Apply commands scheduled for this tick (ownership-checked).
2. Resolve ship tasks whose `eta_tick` has arrived.
3. Tick station modules (refinery processors).
4. Advance station research on all eligible techs.
5. Replenish scan sites if below threshold.
6. Increment `state.meta.tick`.

**Key design rules:**
- Asteroids do not exist in state until surveyed. `state.scan_sites` holds pre-generated sites; survey completion creates and inserts an `AsteroidState`.
- Research is fully automatic — compute distributes evenly across all eligible techs (prereqs met, not yet unlocked). No player allocation.
- DeepScan commands are silently dropped if no unlocked tech has the `EnableDeepScan` effect.
- All collection iteration is sorted by ID before RNG consumption to guarantee determinism.
- Scan sites are replenished each tick when count drops below threshold (MIN_UNSCANNED_SITES=5). New sites use deterministic UUIDs from the seeded RNG.

### `crates/sim_control` (lib)
Command sources. Implements the `CommandSource` trait.

- `AutopilotController`: per-idle-ship priority loop + station module management:
  - **Station pre-pass:** auto-install modules from station inventory, enable disabled modules, set default thresholds on processors.
  - **Ship priorities:**
    1. Deposit cargo at nearest station (if hold non-empty)
    2. Mine best deep-scanned asteroid (sorted by `mass_kg × Fe_fraction` desc)
    3. Deep-scan IronRich asteroids (if `tech_deep_scan_v1` unlocked)
    4. Survey unscanned sites
- `ScenarioSource`: stub — will replay scripted tick→commands from JSON.

### `crates/sim_cli` (bin)
Loads content, builds initial world state, runs the tick loop with autopilot.

- Content loaded from `content/*.json` (7 files assembled into `GameContent`).
- Supports `--state` flag to load a pre-baked JSON state file instead of generating fresh.
- World gen creates 1 station + 1 ship + N scan sites (from templates × `asteroid_count_per_template`).
- Prints a status line every `--print-every` ticks; always prints on tech unlock.

### `crates/sim_daemon` (bin)
axum 0.7 HTTP server that runs the tick loop in a tokio task and streams events to the React UI. Modules: `state`, `world`, `routes`, `tick_loop`.

- Shared state: `Arc<Mutex<SimState>>` (game state + RNG + autopilot).
- Events broadcast via `tokio::sync::broadcast::Sender<Vec<EventEnvelope>>`.
- SSE stream batches events in 50ms flush intervals; heartbeat every 200ms with current tick.
- Tick rate configurable (`--ticks-per-sec`; 0 = as fast as possible).
- Supports `--state` flag to load a pre-baked JSON state file.
- **Endpoints:** `GET /api/v1/meta` (includes `ticks_per_sec`), `GET /api/v1/snapshot`, `GET /api/v1/stream` (SSE).
- CORS configured for `http://localhost:5173` (Vite dev server).

### `ui_web/` (React app — not a Rust crate)
Vite 7 + React 19 + TypeScript 5 + Tailwind CSS v4 mission control dashboard. Tests via Vitest + React Testing Library.

- **Data flow:** on mount fetches `/api/v1/snapshot` to hydrate, then subscribes to `/api/v1/stream` (SSE) for live updates. `applyEvents()` replays events client-side to keep state current without re-fetching snapshots.
- **State:** single `useReducer` in `useSimStream` hook; no external state library.
- **Smooth streaming:** `useAnimatedTick` interpolates display tick at 60fps using `requestAnimationFrame` and measured tick rate from server samples.
- **Layout:** StatusBar (tick/time/rate/connection) + left nav sidebar (panel toggles, persisted to localStorage) + resizable panels (react-resizable-panels v2).
- **Panels:** Map (SVG solar system) | Events | Asteroids (sortable table) | Fleet (sortable ships + stations tables with inventory, module display, task progress bars) | Research.
- **Solar system map:** d3-zoom pan/zoom, orbital rings, entity markers (stations=diamonds, ships=task-colored triangles, asteroids=mass-scaled circles, scan sites=circled `?`), hover tooltips anchored to elements, click-to-select detail cards.
- **Proxy:** `/api` proxied to `http://localhost:3001` in dev (`vite.config.ts`).

## Key Types (sim_core)

| Type | Purpose |
|---|---|
| `GameState` | Full mutable simulation state (meta, scan_sites, asteroids, ships, stations, research, counters) |
| `ScanSite` | Unscanned potential asteroid location (consumed on survey) |
| `AsteroidState` | Created on discovery; holds `true_composition` (hidden), `knowledge`, `mass_kg`, `anomaly_tags` |
| `ResearchState` | `unlocked`, `data_pool`, `evidence` — no active allocations |
| `ShipState` | `id`, `location_node`, `owner`, `inventory: Vec<InventoryItem>`, `cargo_capacity_m3`, `task` |
| `StationState` | `id`, `location_node`, `inventory`, `cargo_capacity_m3`, `power_available_per_tick`, `facilities`, `modules: Vec<ModuleState>` |
| `InventoryItem` | Enum: `Ore { lot_id, asteroid_id, kg, composition }`, `Material { element, kg, quality }`, `Slag { kg, composition }`, `Component { component_id, count, quality }`, `Module { item_id, module_def_id }` |
| `ModuleState` | Installed module: `id`, `def_id`, `enabled`, `kind_state` (Processor or Storage) |
| `TaskKind` | `Idle`, `Survey`, `DeepScan`, `Mine { asteroid, duration_ticks }`, `Deposit { station }`, `Transit { destination, total_ticks, then }` |
| `Command` | `AssignShipTask`, `InstallModule`, `UninstallModule`, `SetModuleEnabled`, `SetModuleThreshold` |
| `GameContent` | Static config: techs, solar system, asteroid templates, elements, module_defs, constants |
| `ModuleDef` | Module definition with `ModuleBehaviorDef` (Processor with recipes, or Storage) |
| `TechEffect` | `EnableDeepScan` or `DeepScanCompositionNoise { sigma }` |

## Content Files

All in `content/`. Loaded at runtime by `sim_cli` and `sim_daemon`; never compiled in.

| File | Key fields |
|---|---|
| `constants.json` | Scan durations, travel ticks, mining rate, cargo capacities, deposit ticks, research compute |
| `techs.json` | Tech tree (`tech_deep_scan_v1` is the only current tech) |
| `solar_system.json` | 4 nodes (Earth Orbit → Inner Belt → Mid Belt → Outer Belt), linear chain |
| `asteroid_templates.json` | 2 templates: `tmpl_iron_rich` (IronRich, Fe-heavy) and `tmpl_silicate` (Si-heavy) |
| `elements.json` | 5 elements: `ore` (3000), `slag` (2500), `Fe` (7874), `Si` (2329), `He` (125) kg/m³ |
| `module_defs.json` | 1 module: `module_basic_iron_refinery` — Processor, 60-tick interval, consumes 1000kg ore, outputs Fe material + slag |
| `dev_base_state.json` | Pre-baked dev state: tick 0, 1 ship, 1 station with refinery module in inventory |

## After Every Change

Tests run automatically via a PostToolUse hook (`.claude/hooks/after-edit.sh`) whenever a `.rs` file is edited — `cargo fmt` then `cargo test`. Fix any failures before moving on.

Additionally, use judgment on the following:

- **If you changed a type, added/removed a crate, or changed tick ordering:** update the relevant section in CLAUDE.md and `memory/MEMORY.md`.
- **If you added a new mechanic or system:** add it to the Key Types table and Architecture section in CLAUDE.md.
- **Before claiming work is complete:** confirm tests pass and no `TODO` stubs were introduced without being noted.

## Inventory & Refinery Design

**Inventory model:** Ships and stations carry `Vec<InventoryItem>` (not HashMap). Volume constraint: `inventory_volume_m3(items, content) ≤ capacity_m3`. Each item type computes volume differently (ore/slag/material by density, components by count, modules by def).

**Ore:** Mining produces `InventoryItem::Ore` with a `lot_id`, `asteroid_id`, `kg`, and snapshot of the asteroid's composition (deep-scanned if available, else true composition). Each asteroid produces distinct ore lots.

**Refinery:** Station modules with `ModuleBehaviorDef::Processor` tick at their defined interval. A processor: checks enabled + power + ore threshold → FIFO-consumes ore up to rate_kg → produces `Material` (element fraction × kg, quality from formula) + `Slag` (remainder). Materials of same element+quality merge. Slag merges into a single accumulating lot.

**Future direction (not yet built):**
- Ore keyed by **composition hash** instead of asteroid ID — compatible ores blend naturally.
- Blending tolerance as a tech unlock: ±2% basic, ±10% advanced.
- Volatiles flag ore as unblendable.
- Component output from processors (type defined but branch is no-op).
- Storage modules (type defined but tick loop skips them).

## MVP Scope

- **MVP-0 (done):** `sim_core` tick + tests, `sim_control` autopilot, `sim_cli` run loop.
- **MVP-1 (done):** `sim_daemon` HTTP server (axum + tokio), SSE event stream, React mission control UI.
- **MVP-2 (done):** Mining loop — cargo holds, Mine/Deposit tasks, ore extraction, autopilot priority reorder, FE fleet/station cargo display.
- **MVP-3 (done):** Refinery system — `Vec<InventoryItem>` inventory model, station modules, processor tick logic, recipe-based ore→material+slag conversion, module commands, autopilot auto-install, FE inventory display with material/slag/module rendering.
- **FE Foundations (done):** Left nav sidebar with panel toggles (localStorage-persisted), `useSortableData` hook, sortable columns on AsteroidTable and FleetPanel, `SortIndicator` component, color refresh (semantic color tokens: accent, cargo, active, online, offline, etc.).
- **Solar System Map (done):** SVG orbital map as a standard panel, d3-zoom pan/zoom, entity markers, hover tooltips, click-to-select detail cards.
- **Smooth Streaming (done):** `useAnimatedTick` hook for 60fps client-side tick interpolation, 200ms heartbeat interval, measured tick rate display in StatusBar, smooth ship transit animation.

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
