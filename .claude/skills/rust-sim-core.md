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
- [ ] **No hardcoded content IDs:** business logic must not branch on specific element, component, or tech ID strings — use content-defined fields
- [ ] **Data-driven types:** game content types (asteroid tags, data kinds, research domains) are Strings loaded from content — never add new enum variants for content-defined categories
- [ ] **Module extensibility:** new module types use common fields/trait — never add match arms to station/mod.rs dispatcher functions

- [ ] **Function size:** new functions over 80 lines should use the accumulator pattern (like `MetricsAccumulator`) or extract sub-functions. Never add `#[allow(clippy::too_many_lines)]` to suppress growth — decompose instead.
- [ ] **Progression tests:** at least one test per major system should use `load_content("../../content")` with real content values (real wear rates, real timescales) — not just zero-wear fixtures

## Rust Analyzer Tools
Use `rust_analyzer_*` MCP tools for Rust navigation — they understand semantics where grep cannot:
- **Navigating types:** `rust_analyzer_definition` to jump to `GameState`, `Event`, `ModuleBehaviorDef`, etc. instead of grepping
- **Finding callers:** `rust_analyzer_references` to find all callers of `tick()`, all uses of a type across crates
- **Checking signatures:** `rust_analyzer_hover` for quick type info without reading surrounding code
- **File overview:** `rust_analyzer_symbols` to see what functions/structs a file contains with line ranges

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
- `AnomalyTag`, `DataKind`, `ResearchDomain` are data-driven (String/newtype), not compile-time enums — don't add variants, add content JSON entries
- Adding a new `ModuleBehaviorDef` variant? Use common fields/trait, don't add 5 match arms to dispatcher functions
