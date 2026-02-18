# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Space industry simulation game. Deterministic Rust sim core, CLI runner, future React UI.
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

# Run the simulation
cargo run -p sim_cli -- run --ticks 1000 --seed 42
cargo run -p sim_cli -- run --ticks 500 --seed 42 --print-every 50 --event-level debug
```

## Architecture

Cargo workspace with three crates. Dependency order: `sim_core` ← `sim_control` ← `sim_cli`.

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

- `AutopilotController`: surveys unscanned sites in order, then deep-scans IronRich asteroids once the tech is unlocked.
- `ScenarioSource`: stub — will replay scripted tick→commands from JSON.

### `crates/sim_cli` (bin)
Loads content, builds initial world state, runs the tick loop with autopilot.

- Content loaded from `content/*.json` (4 files assembled into `GameContent`).
- World gen creates 1 station + 1 ship + N scan sites (from templates × `asteroid_count_per_template`).
- Prints a status line every `--print-every` ticks; always prints on tech unlock.

## Key Types (sim_core)

| Type | Purpose |
|---|---|
| `GameState` | Full mutable simulation state |
| `ScanSite` | Unscanned potential asteroid location (consumed on survey) |
| `AsteroidState` | Created on discovery; holds `true_composition` (hidden) and `knowledge` |
| `ResearchState` | `unlocked`, `data_pool`, `evidence` — no active allocations |
| `TaskKind` | `Idle`, `Survey { site }`, `DeepScan { asteroid }` |
| `Command` | `AssignShipTask { ship_id, task_kind }` only in MVP-0 |
| `GameContent` | Static config: techs, solar system, asteroid templates, constants |
| `TechEffect` | `EnableDeepScan` or `DeepScanCompositionNoise { sigma }` |

## Content Files

All in `content/`. Loaded at runtime by `sim_cli`; never compiled in.

| File | Key fields |
|---|---|
| `constants.json` | Scan durations, data amounts, detection probability, station config |
| `techs.json` | Tech tree (`tech_deep_scan_v1` is the only MVP-0 tech) |
| `solar_system.json` | Nodes and edges (static graph) |
| `asteroid_templates.json` | Composition ranges and anomaly tags per template |

## MVP Scope

- **MVP-0 (done):** `sim_core` tick + tests, `sim_control` autopilot, `sim_cli` run loop.
- **MVP-1 (next):** `sim_daemon` HTTP server (axum + tokio), SSE event stream, React UI.

## Notes

- IDE: RustRover (JetBrains)
- RNG: `ChaCha8Rng` (portable, seeded) in `sim_cli`; `sim_core` takes `&mut impl rand::Rng` to stay IO-free
- Mutation testing with `cargo-mutants` is part of the workflow
