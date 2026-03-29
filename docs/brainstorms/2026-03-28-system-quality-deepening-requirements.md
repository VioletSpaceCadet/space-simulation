---
date: 2026-03-28
topic: system-quality-deepening
---

# System Quality Deepening

## Problem Frame

The simulation has 8+ systems (mining, refining, manufacturing, research, thermal, crew, economy, wear) that each work correctly in isolation but create limited strategic tension with each other. Crew costs nothing after recruitment. Research gates features but doesn't affect throughput. Wear is a boolean degradation, not a sliding scale. These systems run in parallel without compounding — a player managing one system well gets no advantage in others.

Separately, the test infrastructure carries significant boilerplate debt (~30 duplicate 15-field ModuleDef literals), and hot-path inventory lookups use O(n) linear scans.

## Requirements

### R1. Module Efficiency Curves

Replace the current boolean gating system (`should_run()` checks: power_stalled? crew_satisfied? wear_band?) with a continuous 0.0–1.0 efficiency multiplier per module per tick.

- R1.1. Each contributing factor produces a factor in [0.0, 1.0]:
  - **Crew factor**: `assigned / required` (capped at 1.0). A smelter needing 2 operators with 1 assigned runs at 0.5.
  - **Wear factor**: Current `wear_efficiency()` already returns a float — use it directly instead of band-gating.
  - **Power factor**: If power-stalled, factor = 0.0. If not, factor = 1.0. (Future: partial power could yield partial factor.)
  - **Thermal factor**: Existing `thermal_efficiency()` already returns a float — wire it in.
- R1.2. Combined efficiency = product of all factors. Applied to processor/assembler output quantity (yield scales with efficiency).
- R1.3. Modules with efficiency < 1.0 emit a new `ModuleEfficiencyChanged` event with the breakdown, so the FE can display it.
- R1.4. A module with efficiency = 0.0 still doesn't run (preserves current stall behavior for edge cases like zero power).
- R1.5. This replaces the `crew_satisfied` boolean on ModuleState — it becomes redundant since crew_factor handles it continuously.

### R2. Crew Operating Costs

Add ongoing salary costs for stationed crew, creating economic pressure that compounds with trade revenue.

- R2.1. Each crew role has a `salary_per_hour` field in `crew_roles.json` (content-driven, already has `recruitment_cost`).
- R2.2. Each tick: `salary_drain = sum(station.crew[role] * role.salary_per_hour * minutes_per_tick / 60)`. Deducted from `state.balance`.
- R2.3. If balance reaches zero, emit `StationBankrupt` event. No crew strike mechanic for now — just the economic pressure of watching the balance drain.
- R2.4. Salary costs appear in the economy panel alongside trade revenue, creating a visible P&L.
- R2.5. Autopilot `CrewRecruitment` behavior should factor salary burn rate — don't hire crew that will bankrupt the station.

### R3. ModuleDef Test Builder

Replace repeated 15-field `ModuleDef` struct literals across ~30 test files with a builder pattern.

- R3.1. Add `ModuleDef::test(id: &str)` constructor that sets all fields to sensible defaults (mass 0, volume 0, no thermal, no crew, no ports, etc).
- R3.2. Provide chainable methods: `.with_behavior()`, `.with_thermal()`, `.with_crew()`, `.with_ports()`, `.with_power()`, `.with_roles()`.
- R3.3. Migrate all existing test `ModuleDef` literals to use the builder.
- R3.4. Future ModuleDef field additions only need to touch the builder default, not 60+ test sites.

### R4. Inventory Lookup Index

Replace O(n) `station.inventory.iter().find()`/`.position()` calls in commands.rs with indexed lookups.

- R4.1. Add a module-by-ID index to StationState (or use existing `module_type_index` pattern) for O(1) module lookup by `ModuleInstanceId`.
- R4.2. For inventory: add a side-index `HashMap<String, Vec<usize>>` mapping element/component IDs to inventory positions.
- R4.3. Invalidate indexes on inventory mutation (push, remove, swap_remove).
- R4.4. Convert the 12 `.iter().find()`/`.position()` calls in commands.rs to use the index.

## Success Criteria

- R1: A smelter with 1/2 crew produces ~50% output. Running a sim_bench scenario shows meaningful throughput differences based on staffing levels.
- R2: A 30-day sim_bench run shows balance declining due to crew salaries. Stations with more crew drain faster.
- R3: Adding a new field to `ModuleDef` requires editing only `test_fixtures.rs`, not 30+ test files.
- R4: `commands.rs` has zero `.iter().find()` or `.iter().position()` calls on inventory or module lists.

## Scope Boundaries

- **Not** adding new module types, recipes, or tech effects
- **Not** changing the tick order or adding new tick steps
- **Not** adding UI panels — only updating existing panels with new data (efficiency breakdown, salary costs)
- **Not** implementing partial power (R1 has a slot for it but defaults to binary for now)
- **Not** implementing crew strikes or morale — just salary drain

## Key Decisions

- **Continuous efficiency over boolean gates**: Multiplicative factors create emergent compound behavior (a module with bad crew AND bad wear is much worse than either alone). This is more interesting than additive penalties.
- **Salary as economic pressure, not punishment**: No crew strike mechanic — the pressure comes from watching the P&L, not from systems shutting down.
- **Builder in test_fixtures, not derive macro**: A derive macro is overengineered for test-only code. A builder with sensible defaults is simpler and more readable.

## Dependencies / Assumptions

- R1 depends on the existing `wear_efficiency()` and `thermal_efficiency()` functions being correct (they are — well-tested)
- R2 depends on crew_roles.json having salary data (needs content addition)
- R3 is independent — can be done first to reduce friction for R1/R2

## Resolved Questions

- **R1 efficiency target**: Output quantity, not processing time. Module runs at normal interval, yield scales with efficiency. Matches existing `thermal_efficiency()` pattern.
- **R1 FE display**: Single percentage on module card. Hover/expand shows factor breakdown (crew, wear, power, thermal).

## Outstanding Questions

### Deferred to Planning

- [Affects R4][Needs research] Is the module_type_index pattern sufficient for module-by-ID lookup, or does a separate HashMap make more sense?

## Next Steps

→ `/ce:plan` for structured implementation planning
