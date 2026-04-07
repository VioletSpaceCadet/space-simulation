---
title: "P6 AI Intelligence & Optimization — architectural patterns"
category: patterns
date: 2026-04-06
tags:
  - multi-station
  - strategy-config
  - ship-assignment
  - content-driven
  - world-gen
  - autopilot
  - bayesian-optimization
  - mcp-knowledge
  - p6
problem_type: architecture-decision
component: sim_core, sim_control, sim_world, sim_bench, scripts/analysis, mcp_advisor
severity: low
related:
  - docs/solutions/patterns/strategy-consumption-wiring-patterns.md
  - docs/solutions/patterns/autopilot-strategic-layer-foundation-patterns.md
  - docs/solutions/patterns/cross-station-autopilot-coordination.md
  - docs/solutions/patterns/hierarchical-agent-decomposition.md
  - docs/solutions/patterns/scoring-and-measurement-pipeline.md
  - docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md
---

## Context

P6 (AI Intelligence & Optimization) shipped 10 tickets across 3 milestones:
- **M1:** StrategyConfig wiring + consumption (VIO-481, 607, 608, 609, 606)
- **M2:** Multi-station world gen + station-scoped assignment (VIO-485, 487, 489)
- **M3:** Bayesian optimization + knowledge maturity (VIO-610, 612)

This doc captures the cross-cutting architectural patterns. For strategy consumption details (priority halving, phase presets, budget scaling), see `strategy-consumption-wiring-patterns.md`.

## Pattern 1: Content-driven multi-station world generation

**Problem:** Single hardcoded station in `build_initial_state()`. Adding stations required code changes.

**Solution:** New `initial_stations.json` content file with `StationSetupDef` / `ShipSetupDef` types. `build_initial_state()` iterates over setups; falls back to single-station when the array is empty (backward compat).

**Key design decisions:**

1. **Module item ID offsets** prevent cross-station collisions:

```rust
fn build_station_from_setup(
    setup: &StationSetupDef,
    content: &GameContent,
    module_id_offset: usize,
) -> (StationId, StationState, Vec<(ShipId, ShipState)>) {
    // Each module gets a globally unique ID:
    // module_item_{offset + index + 1}
    for (index, module_def_id) in setup.initial.modules.iter().enumerate() {
        inventory.push(InventoryItem::Module {
            item_id: ModuleItemId(format!("module_item_{:04}", module_id_offset + index + 1)),
            ..
        });
    }
}
```

The caller tracks cumulative offset across stations:

```rust
let mut module_id_offset = 0;
for setup in &content.initial_stations {
    let (sid, station, ships) = build_station_from_setup(setup, content, module_id_offset);
    module_id_offset += setup.initial.modules.len();
    assert!(stations.insert(sid.clone(), station).is_none(),
        "Duplicate station ID: {}", sid.0);
    // ...
}
```

2. **Assert on duplicate IDs at load time.** `BTreeMap::insert()` silently overwrites. Content-defined ID maps use `assert!(map.insert(k, v).is_none())` to catch duplicates early.

3. **Dual-path state initialization.** `build_initial_state()` (code path) and `dev_advanced_state.json` (hand-authored) must stay manually consistent. A drift test catches divergence, but it must check ALL dimensions: station count, module types per station, ship assignments — not just the happy path.

## Pattern 2: Pre-partitioned ship assignment

**Problem:** Each station agent scanned all ships to find its own — O(S*K) per tick where S = stations, K = ships.

**Solution:** Pre-partition once at the top of `generate_commands()`:

```rust
fn partition_ships_by_station(
    state: &GameState,
    owner: &PrincipalId,
) -> BTreeMap<StationId, Vec<ShipId>> {
    let mut map: BTreeMap<StationId, Vec<ShipId>> = BTreeMap::new();
    for ship in state.ships.values() {
        if ship.owner != *owner { continue; }
        if let Some(home) = &ship.home_station {
            map.entry(home.clone()).or_default().push(ship.id.clone());
        }
    }
    map
}
```

Each station agent receives only its slice: `home_ships: &[ShipId]`. Ships without `home_station` are excluded — the fleet coordinator handles them separately.

**Trap:** Tests must set `home_station` on ships. A ship with `home_station: None` silently disappears from all station agents' assignment pools.

## Pattern 3: Bayesian optimization over StrategyConfig

**Problem:** Grid search (VIO-528) scales exponentially with parameter count. 18 StrategyConfig parameters make exhaustive search infeasible.

**Solution:** Optuna TPE (Tree-structured Parzen Estimator) sampler. Each trial:
1. Suggests parameters via `suggest_strategy_params(trial)` — defines the 18-parameter space with dotted `strategy.*` keys
2. Creates a temporary scenario JSON with strategy overrides
3. Runs `sim_bench` as a subprocess, parses the composite score
4. Returns the score as the optimization objective

**Optional dependency pattern:** Optuna is optional (not all environments need it):

```python
try:
    import optuna
except ImportError:
    optuna = None  # type: ignore[assignment]
```

The `type: ignore[assignment]` silences mypy when optuna IS installed (it sees `Module` vs `None` type mismatch). When optuna is missing, `import-not-found` fires first. CI installs optuna; the ignore is always needed.

**CI dependency sync:** Python deps are declared in both `pyproject.toml` (local dev) and `.github/workflows/ci.yml` (CI pip install). Adding a dependency is a two-file change until CI is updated to install from `pyproject.toml` directly.

## Pattern 4: Cross-layer strategy context

**Problem:** MCP advisor tools (TypeScript) had no visibility into the Rust simulation's strategy state. Knowledge system couldn't filter by game phase.

**Solution:** Three-language pipeline:

1. **Rust** (`sim_daemon/analytics.rs`): `StrategyContext` struct on `AdvisorDigest`:
```rust
pub struct StrategyContext {
    pub mode: String,
    pub phase: String,
    pub fleet_size_target: u32,
    pub priorities: BTreeMap<String, f32>,
}
```
Populated from `GameState` in the digest handler.

2. **TypeScript** (`mcp_advisor/src/types.ts`): `RunJournal` extended with optional `game_phase` and `strategy_mode` fields. Journals capture which phase/mode was active during observation.

3. **TypeScript** (`mcp_advisor/src/index.ts`): `query_knowledge` tool gained a `phase` filter parameter. Queries can now return only journals from a specific game phase.

This enables phase-aware knowledge retrieval: "show me all observations from the Industrial phase" filters at query time rather than requiring the caller to scan all journals.

## Prevention strategies

### Test fixture drift

Adding a field to `GameContent` or `ShipState` breaks every manual constructor call. **Centralize construction in `test_fixtures.rs`** — never construct these structs inline in tests. Use `..test_content()` to inherit defaults so tests are resilient to new fields.

When adding a field to a widely-used struct, use `rust_analyzer_references` on the struct name to find all construction sites before pushing.

### Dual-path state divergence

The most robust fix is to generate `dev_advanced_state.json` from `build_initial_state()` output with manual overrides. Until then, treat new resources/materials as a three-site change: content definition, `build_initial_state()`, and `dev_advanced_state.json`. The drift test is a safety net, not a substitute for discipline.

### Ship ownership invariants

Every active ship should have a `home_station`. In debug builds, consider asserting this at tick start. Test both "all ships homed" and "ship without home_station" scenarios to verify the fleet coordinator handles both paths.

### BTreeMap silent overwrites

For content-defined ID maps, always use `assert!(map.insert(key, value).is_none())`. This turns silent data loss into a loud panic at load time. A dedicated `insert_unique()` helper would give better error messages.

### Clippy line limits

Watch the diff, not just the result. When a PR adds 30+ lines to an existing function, that is the moment to extract a helper — before clippy catches it at 100 lines. The accumulator pattern (`build_all_stations` calling `build_station_from_setup`) is the right shape from day one.

## Related documentation

- **[strategy-consumption-wiring-patterns.md](strategy-consumption-wiring-patterns.md)** — Companion doc for priority halving, phase presets, budget scaling
- **[autopilot-strategic-layer-foundation-patterns.md](autopilot-strategic-layer-foundation-patterns.md)** — StrategyConfig types, rule interpreter, cache gating (P6 M1 foundation)
- **[cross-station-autopilot-coordination.md](cross-station-autopilot-coordination.md)** — Fleet coordination, hull tag filtering (P5/P6 overlap)
- **[hierarchical-agent-decomposition.md](hierarchical-agent-decomposition.md)** — Agent architecture, execution order, scoping
- **[scoring-and-measurement-pipeline.md](scoring-and-measurement-pipeline.md)** — P0 scoring pipeline that Bayesian optimization builds on
- **[gameplay-deadlock-missing-starting-equipment.md](../logic-errors/gameplay-deadlock-missing-starting-equipment.md)** — Prior art on `build_initial_state()` / `dev_advanced_state.json` divergence
