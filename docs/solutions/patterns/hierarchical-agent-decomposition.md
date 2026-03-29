---
title: "Hierarchical Agent Decomposition: Flat Behaviors → Per-Entity Agents"
category: patterns
date: 2026-03-29
tags: [architecture, agents, refactoring, sim_control, autopilot, scoping]
tickets: [VIO-445, VIO-446, VIO-447, VIO-448, VIO-449, VIO-450, VIO-451, VIO-452, VIO-453, VIO-454]
---

# Hierarchical Agent Decomposition

## Problem

The autopilot was a flat monolithic controller with 9 behaviors (`Vec<Box<dyn AutopilotBehavior>>`), each iterating ALL entities globally. This made scoping unclear, prevented per-entity state (caches), and blocked future strategic/tactical layering.

## Solution

Decompose into per-entity agents: `StationAgent` (per-station, 9 sub-concerns) and `ShipAgent` (per-ship, tactical execution). Station agents set objectives for ship agents — layers communicate via typed `ShipObjective` enum, not direct commands.

### Architecture

```
AutopilotController
├── station_agents: BTreeMap<StationId, StationAgent>
│   └── generate() → station commands (modules, labs, crew, trade)
│   └── assign_ship_objectives() → sets ShipObjective on co-located ships
└── ship_agents: BTreeMap<ShipId, ShipAgent>
    └── generate() → tactical commands (transit, mine, deposit, refuel)
```

### Execution Order

1. Sync agent lifecycle (create/remove for new/deleted entities)
2. Station agents `generate()` in BTreeMap order (deterministic)
3. Station agents `assign_ship_objectives()` with shared-iterator deduplication
4. Ship agents `generate()` in BTreeMap order (deterministic)

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

The flat `ShipAssignmentBridge` ran once globally — its shared iterators ensured no two ships targeted the same asteroid. When moved to per-station agents, deduplication became per-station. With multiple stations, two stations could assign the same asteroid. Acceptable for single-station game, but documented with a comment pointing to the strategic layer for future multi-station support.

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

## Prevention

- PR review checklist should include "scoping audit" for any per-entity agent work
- At least one test per agent method should use `load_content("../../content")` with production values, not zero-value fixtures
- When extracting methods from global behaviors, add a `// Scope: per-station` or `// Scope: global` comment to each query
