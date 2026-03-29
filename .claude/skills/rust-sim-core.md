---
name: Rust Simulation Core
triggers: [sim_core, tick, determinism, GameState, RNG, inventory, wear, research, asteroid, mining, sim_world, sim_control, autopilot, instrumentation, TickTimings, timed]
agents: [sim-e2e-tester, perf-reviewer]
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

- [ ] **Instrumentation:** new tick steps or station sub-steps must be wrapped in `timed!(timings, field_name, expr)`. Add a corresponding `Duration` field to `TickTimings` and update `iter_fields()`. The macro is zero-cost when `None` — no `Instant::now()` calls.
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

## Instrumentation (`sim_core::instrumentation`)
- `TickTimings`: 14 `Duration` fields — 6 top-level tick steps + 8 station sub-steps (aggregated across stations)
- `timed!(timings, field, expr)`: cfg-gated macro. Active in debug or with `instrumentation` feature. Body executes always; timing recorded only when `timings.is_some()`.
- `tick()` signature: `tick(state, commands, content, rng, event_level, timings: Option<&mut TickTimings>)` — pass `None` for zero-cost
- `compute_step_stats(&[TickTimings]) -> Vec<StepStats>`: shared stats computation (mean/p50/p95/max µs)
- Adding a new tick step? Add a `Duration` field to `TickTimings`, update `iter_fields()`, wrap with `timed!`
- The `timed!` macro discards the expression's return value — only use with `()` expressions

## Pitfalls
- `matches!(x, None | Some(t) if ...)` doesn't bind `t` for the None arm — use `.map_or(true, |t| ...)`
- Base test content has empty `domain_requirements`; tests asserting tech NOT unlocked need unmet requirements (e.g., `Manufacturing: 1_000_000.0`)
- Asteroids are created on discovery (from `scan_sites`), not pre-populated
- Raw data lives on `ResearchState` (sim-wide), not station inventory
- Research unlock is deterministic: tech unlocks when all domain requirements are met
- `DeepScan` commands silently dropped if no unlocked tech has `EnableDeepScan` effect
- `AnomalyTag`, `DataKind`, `ResearchDomain` are data-driven (String/newtype), not compile-time enums — don't add variants, add content JSON entries
- Adding a new `ModuleBehaviorDef` variant? Use common fields/trait, don't add 5 match arms to dispatcher functions
