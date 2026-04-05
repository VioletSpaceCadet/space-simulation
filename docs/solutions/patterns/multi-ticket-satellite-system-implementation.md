---
title: "P4 Satellite System — multi-ticket implementation patterns"
category: patterns
date: 2026-04-05
tags:
  - satellites
  - autopilot
  - concern-based-architecture
  - content-driven-dispatch
  - serde-flatten
  - function-decomposition
  - metrics-propagation
  - milestone-counters
  - scoring-dimensions
  - sim-bench
components:
  - sim_core
  - sim_control
  - sim_bench
  - sim_world
  - sim_daemon
severity: reference
related:
  - docs/solutions/patterns/hierarchical-agent-decomposition.md
  - docs/solutions/patterns/scoring-and-measurement-pipeline.md
  - docs/solutions/patterns/progression-system-implementation.md
  - docs/solutions/patterns/cross-layer-feature-development.md
  - docs/solutions/patterns/content-driven-event-engine.md
tickets:
  - VIO-572
  - VIO-575
  - VIO-576
  - VIO-577
  - VIO-578
---

# P4 Satellite System Implementation Patterns

5 tickets implementing satellite deployment, autopilot management, milestones,
bench scenarios, and scoring extension across `sim_core`, `sim_control`,
`sim_bench`, and `sim_daemon`. PRs #421, #423, #426, #429, #431.

## Pattern 1: Extending an Enum-Dispatched Command System

**Problem:** Adding `LaunchPayload::Satellite` to an existing launch system
and `Command::DeploySatellite` for orbital deploy, without duplicating
construction logic.

**Solution:** Add the variant, then extract a shared `pub(crate)` constructor
in `engine.rs` that both the launch transit handler and the new deploy command
call.

```rust
// engine.rs — shared constructor, called from both resolve_transit_payload and handle_deploy_satellite
pub(crate) fn create_satellite(
    satellite_def_id: &str,
    position: Position,
    current_tick: u64,
    content: &GameContent,
    rng: &mut impl Rng,
) -> Option<SatelliteState> { ... }
```

**Gotcha:** `ComponentId` uses the `string_id!` macro. Access the inner
`String` via `.0`, not `.as_str()` — the latter doesn't exist on the newtype.

## Pattern 2: Function Decomposition for Clippy Line Limits

**Problem:** `handle_launch` grew to 166 lines after adding satellite
validation. Clippy's `too_many_lines` lint rejects functions over 100 lines.

**Solution:** Extract exactly the sub-responsibilities into named helpers
*before* adding the new variant. Five helpers extracted:

| Helper | Responsibility |
|--------|---------------|
| `find_available_pad` | Search facility modules for an available pad |
| `validate_satellite_payload` | Check satellite def, tech, and component |
| `compute_payload_mass` | Dispatch mass calculation by payload type |
| `resolve_transit_payload` | Handle completed transit by payload type |
| `create_satellite` | Construct a `SatelliteState` from def + position |

Each helper has a single job, 10-30 lines. Never add
`#[allow(clippy::too_many_lines)]` — decompose instead.

## Pattern 3: GroundFacilityConcern for Autopilot

**Problem:** Autopilot needs to autonomously manage satellite fleet: import
components, launch satellites, replace aging ones.

**Solution:** New `SatelliteManagement` struct implementing
`GroundFacilityConcern` trait, inserted after `ComponentPurchase` and before
`LaunchExecution` in the concern chain.

Key design decisions:
- **Priority ordering:** comm relay -> survey -> nav -> science (content-driven
  via `AutopilotConfig.satellite_priority`)
- **Wear-based replacement:** Queue replacement when wear exceeds
  `satellite_replacement_wear` threshold (default 0.7)
- **One action per tick:** Either import OR launch, not both — prevents budget
  spikes
- **In-transit dedup:** Check `launch_transits` before issuing a duplicate
  launch

**Reviewer-caught bug:** Initial implementation skipped pad availability check.
The concern would generate a doomed `Launch` command every tick when a satellite
component was in inventory but no pad was free. Fix: mirror the pad check from
`LaunchExecution`.

**AutopilotConfig additions** (all `serde(default)`):
```rust
pub satellite_priority: Vec<String>,        // deployment order
pub satellite_launch_rocket: String,        // preferred rocket ID
pub satellite_replacement_wear: f64,        // default 0.7
pub satellite_tech: String,                 // gate tech
```

## Pattern 4: ComponentDef Registration for Trade System

**Problem:** Satellite products need `ComponentDef` entries for
`compute_mass()` in the trade system, in addition to `SatelliteDef` entries.
Missing `ComponentDef` causes `compute_import_cost()` to return `None`
silently.

**Checklist when adding a new manufactured product:**
1. Add to `content/component_defs.json` (mass_kg, volume_m3)
2. Add to `content/pricing.json` (importable/exportable)
3. Add to appropriate def file (`satellite_defs.json`, etc.)
4. Verify `compute_mass()` resolves in a test

## Pattern 5: Milestone Counter Extensibility

**Problem:** Adding satellite-related milestone counters.

**Solution:** Add match arms to `resolve_counter()` in `milestone.rs`. All
satellite counters filter by `s.enabled` — disabled/failed satellites don't
count.

```rust
"satellites_deployed" => Some(state.satellites.values().filter(|s| s.enabled).count() as f64),
"comm_satellites" => Some(state.satellites.values()
    .filter(|s| s.enabled && s.satellite_type == "communication").count() as f64),
```

Chained milestones (e.g., `deep_space_comm` depends on `first_comm_relay`)
work automatically via the multi-pass evaluation loop — no special wiring.

## Pattern 6: JSON State Files and serde Format Rules

**Problem:** Creating `satellite_start.json` for bench scenarios. Three serde
format mismatches caused deserialization failures.

| Struct attribute | JSON format | Common mistake |
|-----------------|-------------|----------------|
| `#[serde(flatten)]` on `core: FacilityCore` | Fields at parent level | Nesting inside `"core": {}` |
| `enum ModuleKindState` (no tag attr) | `{"LaunchPad": {...}}` | `{"type": "LaunchPad", ...}` |
| `WearState` struct field | `"wear": 0.0` | `"level": 0.0` |

**Rule:** Never write state JSON from memory. Serialize an existing in-memory
state via `serde_json::to_string_pretty` first, then use as template.

## Pattern 7: MetricsSnapshot Field Propagation

**Problem:** Adding `satellites_active` and `satellites_failed` to
`MetricsSnapshot` requires updating ~6 constructor sites across `sim_core`,
`sim_bench`, and `sim_daemon`.

**Solution:** Grep for `MetricsSnapshot {` to find all constructor sites. Update
atomically — a partial update compiles but produces incorrect metrics silently.

Sites updated: `scoring.rs` (test), `summary.rs` (test), `parquet_writer.rs`
(test), `run_result.rs` (struct + from_snapshot + test), `analytics.rs` (test),
`alerts.rs` (test).

**Scoring weight rebalancing:**
- Expansion: stations 40% / fleet 30% / satellites 30%
- Fleet ops: utilization 50% / growth 30% / satellite deployments 20%
- Research: tech unlocks 60% / data 25% / science satellites 15%
- Efficiency: 4-way equal blend (wear, power, storage, satellite utilization)

When no satellites exist, `sat_util` defaults to `1.0` (neutral) — doesn't
penalize pre-satellite players.

## Prevention Checklist

### Before adding a new field to a widely-constructed struct
- [ ] Grep for `StructName {` across the workspace — count matches
- [ ] Update ALL constructor sites in one pass
- [ ] `cargo build --workspace` to verify (missing fields are compile errors)

### Before adding a new autopilot concern
- [ ] What resource does the command consume? (pad, ship, crew)
- [ ] Is there an availability check gating command generation?
- [ ] Test: concern emits zero commands when resource at capacity
- [ ] Test: concern emits commands when resource available

### Before adding a new manufactured product
- [ ] Entry in `component_defs.json` (mass + volume)
- [ ] Entry in `pricing.json` (import/export flags)
- [ ] Entry in product-specific defs (satellite_defs, etc.)
- [ ] `compute_mass()` test for the new component ID

### Before writing JSON state files
- [ ] Check `#[serde(flatten)]` on any composed struct
- [ ] Check enum tag format (externally vs internally tagged)
- [ ] Verify field names match struct definitions exactly
- [ ] Round-trip test: deserialize and re-serialize

### Before bulk string replacement
- [ ] Is `old_string` a substring of `new_string`? If so, not idempotent
- [ ] After replace: grep for doubled prefixes (`crate::crate::`)
- [ ] `cargo build` immediately after to catch path resolution errors
