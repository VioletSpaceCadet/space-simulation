---
title: "Extending Entity Type Coverage in Metrics, Milestones & Scoring"
category: patterns
date: 2026-04-05
tags:
  - ground-operations
  - milestones
  - scoring
  - metrics
  - counters
  - progression
  - content-threading
  - p2-ground-ops
components:
  - sim_core/milestone.rs
  - sim_core/metrics.rs
  - sim_core/scoring.rs
  - sim_core/engine.rs
  - content/milestones.json
problem_type: multi-ticket-implementation
severity: low
---

## Problem

P2 introduced ground facilities as a new gameplay start path, but the simulation's measurement, progression, and scoring layers were entirely station-centric. Three failure modes:

1. **Milestones never fired** for ground-start runs because counters only inspected station inventories.
2. **Scoring penalized ground starts** (composite 322 vs orbital ~545) because metrics, research scoring, economic baseline, and expansion counting all ignored ground facilities.
3. **No sim_bench validation** existed to catch regressions in ground-start balance.

## Root Cause

The code that counted "things that happened" — milestones, metrics, scoring — predated ground facilities and had explicit station-only assumptions:

- `assembler_runs` counter only walked `state.stations`
- `orbital_stations` used `stations.len()` which included pre-existing stations from world gen
- Metrics accumulation had no `accumulate_ground_facility` path
- Research scoring hardcoded `DataKind::SURVEY` (ground sensors produce `OpticalData`)
- Economic scoring hardcoded `$1B` starting balance (ground start is `$50M`)
- Expansion scoring counted only `state.stations.len()`

## Solution

### Pattern 1: Thread Content Through Counter Resolution

When counters need to check content-defined keys (e.g., which components are rockets), pass `&GameContent` through the evaluation chain:

```rust
// Before — no content access
fn resolve_counter(state: &GameState, counter: &str) -> Option<f64>

// After — content-aware counters
fn resolve_counter(state: &GameState, content: &GameContent, counter: &str) -> Option<f64>
```

This enabled `rockets_in_inventory` to check `content.rocket_defs.contains_key()` instead of hardcoding component names.

### Pattern 2: Event-Driven Counters, Not Collection Sizes

**Collection size = current state. Counters = history.**

Using `stations.len()` as a "stations deployed" signal is wrong — it includes stations from world gen. Use a dedicated counter incremented only on the triggering event:

```rust
// On Counters struct (with #[serde(default)] for backward compat)
pub stations_deployed: u64,

// In resolve_launch_transits — increment only on StationKit delivery
state.counters.stations_deployed += 1;
```

**Corollary:** Derive `Default` on `Counters` and use `..Default::default()` in test fixtures to avoid breaking ~16 test files when adding new fields.

### Pattern 3: Accumulate All Entity Types in Metrics

When adding a new entity type, add its accumulation path in the same PR:

```rust
pub fn compute_metrics(state: &GameState, content: &GameContent) -> MetricsSnapshot {
    let mut acc = MetricsAccumulator::new();
    for station in state.stations.values() {
        acc.accumulate_station(station, content);
    }
    for facility in state.ground_facilities.values() {
        acc.accumulate_ground_facility(facility, content);  // NEW
    }
    for ship in state.ships.values() {
        acc.accumulate_ship(ship, content);
    }
    acc.finalize(state)
}
```

### Pattern 4: Sum Content-Defined Categories Dynamically

Scoring functions must never branch on specific content IDs. Sum across all values:

```rust
// Wrong — hardcodes one data kind
let data = state.research.data_pool.get(&DataKind::new("Survey")).unwrap_or(&0.0);

// Right — sums all kinds
let total: f64 = state.research.data_pool.values().map(|v| f64::from(*v)).sum();
```

### Pattern 5: Dynamic Normalization Baselines

Don't hardcode normalization denominators. Different starting states need different baselines:

```rust
// Wrong — assumes $1B start
let balance_ratio = state.balance / 1_000_000_000.0;

// Right — adapts to actual balance
let starting_balance = state.balance.max(50_000_000.0);
let balance_ratio = (state.balance / starting_balance).clamp(0.0, 2.0) / 2.0;
```

### Pattern 6: Placeholder Counter Stubs

For deferred features (e.g., VIO-560 reusability), return `Some(0.0)` instead of leaving the counter unresolvable (`None`). This distinguishes "system not implemented yet" from "counter name typo":

```rust
"reusable_landings" => Some(0.0), // Placeholder — VIO-560 deferred
```

### Pattern 7: Milestone Chaining to Avoid Double-Firing

When a new milestone shares a condition with an existing one (e.g., both trigger on `assembler_runs >= 1`), add a prerequisite to chain them:

```json
{
  "id": "first_manufactured_component",
  "conditions": [
    { "type": "milestone_completed", "milestone_id": "first_observation" },
    { "type": "counter_above", "counter": "assembler_runs", "threshold": 1 }
  ]
}
```

## Prevention Checklist

When adding a new entity type to the simulation:

- [ ] **Metrics:** Add accumulation path in `compute_metrics` for the new entity type
- [ ] **Counters:** Use event-driven counters (not collection sizes) for "player did X" milestones
- [ ] **Scoring:** Verify all scoring dimensions handle zero-entity cases gracefully (no divide-by-zero when fleet/stations are empty)
- [ ] **Scoring:** Verify normalization baselines aren't hardcoded to a specific starting state
- [ ] **Research:** Verify data kind aggregation sums all kinds, not just one
- [ ] **Milestones:** Verify no two milestones share identical trigger conditions without chaining
- [ ] **Counters struct:** Derive `Default`, use `#[serde(default)]` on new fields, and `..Default::default()` in tests
- [ ] **sim_bench:** Add at least one smoke scenario for the new start path
- [ ] **CI:** Integrate smoke validation into `ci_bench_smoke.sh`

## Tickets

- VIO-564: Ground operations milestones (PR #430)
- VIO-565: sim_bench ground scenarios (PR #432)
- VIO-566: Score dimension extension (PR #434)

## Cross-References

- [Progression System Implementation](progression-system-implementation.md) — P1 milestone patterns
- [Scoring and Measurement Pipeline](scoring-and-measurement-pipeline.md) — P0 scoring foundation
- [Multi-Ticket Satellite System Implementation](multi-ticket-satellite-system-implementation.md) — P4 patterns for GroundFacilityConcern and MetricsSnapshot field propagation
