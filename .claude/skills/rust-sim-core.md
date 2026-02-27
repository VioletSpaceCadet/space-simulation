---
name: Rust Simulation Core
triggers: [sim_core, tick, determinism, GameState, RNG, inventory, wear, research, asteroid, mining, sim_world, sim_control, autopilot]
agents: [sim-e2e-tester]
---

## When to Use
Any work in `sim_core`, `sim_world`, or `sim_control` — game state, tick logic, determinism, inventory, wear, research, mining, or autopilot.

## Checklist
- [ ] **Determinism:** sort all collection iterations by ID before any RNG use
- [ ] **Tick ordering:** changes respect the order (commands → ships → modules → research → scan sites → increment)
- [ ] **Borrow checker:** build commands/data before calling `tick()` — can't hold `&state` and `&mut state`
- [ ] **RNG threading:** pass `&mut impl rand::Rng`, concrete `ChaCha8Rng` only in cli/daemon
- [ ] **Content loading:** new fields need defaults or `#[serde(default)]` for backward compat
- [ ] **Snapshot fixtures:** if state shape changed, update test fixtures

## Testing
- **Unit:** `cargo test -p sim_core` (runs automatically via PostToolUse hook on `.rs` edits)
- **Scenario:** `cargo run -p sim_bench -- run --scenario scenarios/baseline.json`
- **Mutation:** `cargo mutants -p sim_core` for critical paths
- **Balance:** sim-e2e-tester agent for multi-seed bulk runs

## Pitfalls
- `matches!(x, None | Some(t) if ...)` doesn't bind `t` for the None arm — use `.map_or(true, |t| ...)`
- Test content uses `difficulty=10`; tests asserting tech NOT unlocked need high difficulty (`1_000_000`)
- Asteroids are created on discovery (from `scan_sites`), not pre-populated
- Raw data lives on `ResearchState` (sim-wide), not station inventory
- Research rolls every N ticks, not every tick
- `DeepScan` commands silently dropped if no unlocked tech has `EnableDeepScan` effect
