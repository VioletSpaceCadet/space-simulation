---
title: "Hierarchical Agent Decomposition: Flat Behaviors → Per-Entity Agents"
category: patterns
date: 2026-03-29
last_refreshed: 2026-04-05
tags: [architecture, agents, refactoring, sim_control, autopilot, scoping, strategic-layer]
tickets: [VIO-445, VIO-446, VIO-447, VIO-448, VIO-449, VIO-450, VIO-451, VIO-452, VIO-453, VIO-454, VIO-479, VIO-480]
---

# Hierarchical Agent Decomposition

## Problem

The autopilot was a flat monolithic controller with 9 behaviors (`Vec<Box<dyn AutopilotBehavior>>`), each iterating ALL entities globally. This made scoping unclear, prevented per-entity state (caches), and blocked future strategic/tactical layering.

## Solution

Decompose into per-entity agents: `StationAgent` (per-station, 9 sub-concerns), `GroundFacilityAgent` (per-facility, sensor/budget), and `ShipAgent` (per-ship, tactical execution). A `StrategyRuntimeState` sits above the agents and emits shared `ConcernPriorities` that station agents consume. Station agents set objectives for ship agents — layers communicate via typed `ShipObjective` enum, not direct commands.

### Architecture

```
AutopilotController
├── strategy_runtime: StrategyRuntimeState            // VIO-479/480 strategic layer
│   └── evaluate_strategy(state, content) → ConcernPriorities
│       (gated on interval or dirty flag; hysteresis + temporal bias)
├── station_agents: BTreeMap<StationId, StationAgent>
│   └── generate() → station commands (modules, labs, crew, trade)
│   └── assign_ship_objectives() → sets ShipObjective on home ships
├── ground_facility_agents: BTreeMap<GroundFacilityId, GroundFacilityAgent>
│   └── generate() → facility commands (sensor purchase, budget)
├── fleet_coordinator (VIO-598) → global supply/demand, Transfer objectives
├── module_delivery (VIO-596) → cross-station module transfers for empty stations
└── ship_agents: BTreeMap<ShipId, ShipAgent>
    └── generate() → tactical commands (transit, mine, deposit, refuel, transfer)
```

### Execution Order

0. **Strategic pass** — `evaluate_strategy()` refreshes cached `ConcernPriorities`
   (cache-hit unless interval elapsed or dirty flag set). Runs first so every
   per-entity agent within this tick sees consistent priorities.
1. Sync agent lifecycle (create/remove for new/deleted stations, facilities, ships)
2. Station agents `generate()` in `BTreeMap` order (deterministic)
3. Ground facility agents `generate()` in `BTreeMap` order (deterministic)
3.5a. `fleet_coordinator::evaluate_and_assign()` — global supply/demand, assigns Transfer objectives to idle ships (VIO-598). Prefers logistics-tagged ships, excludes mining-tagged.
3.5b. `module_delivery::assign_module_deliveries()` — cross-station module transfers for empty stations (VIO-596). Same hull filtering as 3.5a.
4. Station agents `assign_ship_objectives()` with shared-iterator deduplication. Excludes logistics-tagged ships (VIO-599).
5. Ship agents `generate()` in `BTreeMap` order (deterministic)

### Strategic Layer Gating (VIO-480)

`evaluate_strategy` is a hot path — it's called every `generate_commands` pass
(every tick in sim_daemon). To keep it cheap:

- **Interval gate**: recomputes only every `STRATEGY_EVAL_INTERVAL_MINUTES`
  (600 min). Between evaluations, returns the cached `ConcernPriorities`.
  The interval is converted via `Constants::game_minutes_to_ticks()` so
  mpt=1 test fixtures behave the same as mpt=60 production.
- **Dirty flag**: `mark_strategy_dirty()` forces a recompute on the next pass.
  The `SetStrategyConfig` command handler (VIO-483) sets this so runtime strategy
  changes take effect immediately instead of waiting for the next gate tick.
- **Hysteresis**: once a concern is "active" (weight ≥ 0.5), it gets an
  `HYSTERESIS_BONUS` next evaluation to prevent flicker between runs.
- **Temporal bias**: per-concern `last_serviced` ticks push neglected concerns
  up, capped at `TEMPORAL_BIAS_MAX` after `TEMPORAL_BIAS_SATURATION_MINUTES`.

## Key Pitfall: Global-to-Per-Entity Scoping

When a flat behavior iterates ALL entities, converting it to a per-entity agent can accidentally scope global queries. **This was caught by PR review, not by tests.**

### Example: Salary Projection

The flat `CrewRecruitment` behavior checked:
```rust
// Flat behavior — iterates ALL stations, sees ALL crew
let current_salary: f64 = station.crew.iter().map(/* ... */).sum();
```

When ported to `StationAgent::recruit_crew()`, this initially only summed the agent's own station's crew. But the engine's `deduct_crew_salaries()` sums across ALL stations. The fix:

```rust
// Per-entity agent — must explicitly query global state
let current_salary_per_tick: f64 = state.stations.values()
    .flat_map(|s| s.crew.iter())
    .map(|(r, &c)| { /* salary calc */ })
    .sum();
```

### Example: Shared-Iterator Deduplication

The flat `ShipAssignmentBridge` ran once globally — its shared iterators ensured no two ships targeted the same asteroid. When moved to per-station agents, deduplication became per-station. With multiple stations, two stations could assign the same asteroid. This is now mitigated by execution ordering: `FleetCoordinator` (step 3.5a) and `module_delivery` (step 3.5b) claim ships before per-station `assign_ship_objectives` (step 4), and hull tag filtering (VIO-599) keeps logistics ships out of mining pools. See [`cross-station-autopilot-coordination.md`](./cross-station-autopilot-coordination.md) for the full coordination pattern.

### Example: Hardcoded Tick Counts

`let projection_ticks: u64 = 720` assumed mpt=60 (30 days). Test fixtures use mpt=1, making this only 12 hours. Fix: `content.constants.game_minutes_to_ticks(30 * 24 * 60)`.

## Checklist: Converting Flat Behavior to Per-Entity Agent

1. **Grep for `state.stations.values()` / `state.ships.values()`** in the behavior — any iteration across ALL entities needs careful scoping review
2. **Check aggregation functions** (`total_element_inventory`, salary sums, module role checks) — do they need to remain global or become per-entity?
3. **Shared iterators** — if deduplication spans entities, document the new scope boundary
4. **Hardcoded tick/time constants** — use `game_minutes_to_ticks()` for tick-rate independence
5. **Test with realistic values** — absurd values (salary = 1M/hr) mask scoping bugs; boundary-value tests catch them

## Concurrent Modification Pattern

When refactoring removes code another branch modifies (VIO-442 modified `CrewRecruitment` while VIO-453 deleted it), create a follow-up ticket (VIO-454) immediately rather than trying to coordinate branches. The follow-up ports the change to the new location after the modifying branch lands.

## Caching Strategy: LabAssignmentCache

Per-entity agents can hold caches that persist across ticks. `LabAssignmentCache` avoids re-computing eligible techs every tick:

- `initialized: bool` — first call rebuilds the cache
- `last_unlocked_count: usize` — invalidates when a tech unlocks (count changes)
- `cached_eligible: Vec<(TechId, f32)>` — sorted eligible techs with sufficiency scores

**Invalidation rule**: compare `state.research.unlocked.len()` against `last_unlocked_count`. If changed, rebuild. This is O(1) to check and avoids the full O(techs × domains) sufficiency computation on 99% of ticks.

## Performance: Early-Exit Optimization (VIO-456)

The agent decomposition initially caused a ~15% tick throughput regression because `generate_commands()` ran all sub-concerns every tick, even when nothing changed. Fix: each sub-concern checks a cheap precondition before doing work.

Examples of early exits in `StationAgent` sub-concerns:
- `assign_crew()`: exits if `!has_unsatisfied_crew_need(station, content)`
- `manage_labs()`: exits if cache is valid and no new techs unlocked
- `import_components()`: exits if trade not yet unlocked

These early exits restored throughput to pre-refactor levels while keeping the clean per-concern method structure.

## Related Patterns

### Strategic Layer Foundation (VIO-479/480, VIO-482–484, VIO-605)

See [autopilot strategic layer foundation patterns](./autopilot-strategic-layer-foundation-patterns.md)
for the full P6 Phase C learnings: rule-interpreter design, cache gating,
content-vs-runtime-state counting, the `Command`+`Event` cross-layer checklist,
and the "safe slice" multi-ticket parallelization strategy. The strategic layer
was added on top of this per-entity agent architecture — it does not replace it.

### ModuleDefBuilder (VIO-436/437)

Builder API for constructing `ModuleDef` in test fixtures. Avoids 20+ field struct literals:

```rust
ModuleDefBuilder::new("module_test")
    .name("Test Module")
    .power(10.0)
    .wear(0.01)
    .crew("operator", 2)
    .behavior(ModuleBehaviorDef::Processor(...))
    .build()
```

All fields have sensible defaults. Chain only the fields your test cares about.

### ModuleTypeIndex (VIO-443/444)

Pre-computed per-type module index vectors on `StationState`. Rebuilt on install/uninstall:

- `processors: Vec<usize>`, `labs: Vec<usize>`, etc.
- Each tick subsystem iterates only its matching indices
- `module_id_index: HashMap<ModuleInstanceId, usize>` for O(1) lookup by ID

**Invalidation**: `rebuild_module_index()` called from `handle_install_module()` and `handle_uninstall_module()` command handlers.

## Prevention

- PR review checklist should include "scoping audit" for any per-entity agent work
- At least one test per agent method should use `load_content("../../content")` with production values, not zero-value fixtures
- When extracting methods from global behaviors, add a `// Scope: per-station` or `// Scope: global` comment to each query
