---
title: Expanding the tech tree via StatModifier — Epic 5 patterns and pitfalls
category: patterns
date: 2026-03-27
tags: [research, modifiers, tech-effects, balance, StatId, power-budget, boiloff, propulsion, battery]
components: [sim_core/modifiers, sim_core/station, sim_core/commands, sim_core/station/boiloff, content/techs.json]
---

## Problem

Epic 5 required adding 5 new tech effects (solar output, power consumption, boiloff rate, fuel efficiency, battery capacity). The original tickets assumed adding 5 new `TechEffect` enum variants with a hand-rolled `tech_effect_multiplier()` helper. The codebase had since evolved to use a generic `StatModifier { stat, op, value }` system — making the original approach obsolete.

## Root Cause (of the design drift)

The tickets were written (Feb 2026) before the Research System Redesign landed. That redesign introduced `TechEffect::StatModifier` with a 4-phase `ModifierSet::resolve()` pipeline and a `StatId` enum covering 17+ game stats. The planned per-effect enum variants were no longer needed.

## Solution: Use StatModifier with new StatId variants

Each tech effect follows the same 3-step pattern:

1. **Add a `StatId` variant** to `modifiers.rs` (e.g., `BoiloffRate`, `FuelEfficiency`, `BatteryCapacity`)
2. **Wire `modifiers.resolve()` into the system** — one line resolving the global modifier, then multiply
3. **Add the tech to `techs.json`** with a `StatModifier` effect targeting the new stat

### Pattern: Resolve once before loop, apply per-item

```rust
// Resolve global modifier ONCE (before any mutable borrows)
let boiloff_rate_mult = state.modifiers.resolve_f32(
    crate::modifiers::StatId::BoiloffRate, 1.0,
);

// Apply inside the per-item loop
let loss = (kg * base_rate * multiplier * f64::from(boiloff_rate_mult)) as f32;
```

This pattern was used for all 5 effects:
- `PowerConsumption` — resolved in `rebuild_power_cache()`, applied to all module consumption and the stalling priority vector
- `BoiloffRate` — resolved before the mutable station borrow in `apply_boiloff()`
- `FuelEfficiency` — resolved after `compute_transit_fuel()` in `deduct_transit_fuel()`
- `BatteryCapacity` — resolved in `rebuild_power_cache()`, applied to cloned `BatteryDef.capacity_kwh`
- `SolarOutput` — already wired via existing `resolve_with_f32()` pattern

### Pitfall: Stat coupling between solar arrays and batteries

Solar arrays and batteries both used `PowerOutput` stat for modifier resolution. Adding a solar efficiency tech targeting `PowerOutput` would have unintentionally boosted battery capacity (battery uses `PowerOutput` to scale its effective capacity via wear efficiency).

**Fix:** Added a dedicated `SolarOutput` StatId for solar arrays, keeping `PowerOutput` for batteries. Always check what OTHER systems resolve the same StatId before adding a tech modifier.

### Pitfall: clippy too_many_lines after adding modifier resolution

Adding the `BatteryCapacity` multiplier to `rebuild_power_cache()` pushed it past clippy's 100-line limit. Extracted `resolve_solar_output()` as a standalone helper — a behavior-preserving refactor.

**Lesson:** When a function is near the line limit, plan the extraction alongside the feature, not as an afterthought CI fix.

## Balance finding: Research evidence accumulation is very slow

Domain requirements needed to be scaled from 100-500 down to 5-50. Root causes:

1. **Data generation is the bottleneck** — labs consume data faster than sensors/miners/assemblers produce it, leaving labs data-starved most of the time
2. **Autopilot lab assignment has an alphabetical tiebreaker** — when all techs have 0 sufficiency (start of game), labs are assigned alphabetically. High-requirement techs (e.g., `tech_advanced_manufacturing` at Mfg 200) get the lab first, starving cheaper techs
3. **Multi-domain techs are disadvantaged** — geometric mean sufficiency means a tech needing Manufacturing 10 + Survey 5 ranks below a tech needing Manufacturing 10 alone, because the Survey domain starts at 0

**Key metric:** Evidence accumulates at ~0.04 points/tick per domain in the full autopilot context (vs theoretical 4-5 points/tick if labs were fully fed). (auto memory [claude]: propellant balance observations noted similar data-rate constraints in Epic 4 verification)

## Prevention

- **When adding a new StatId:** grep for all existing `resolve()` / `resolve_with()` calls using the stat you're coupling to. Verify no unintended cross-system effects.
- **When wiring modifiers into systems with `&mut` borrows:** resolve the modifier BEFORE the mutable borrow to avoid Rust borrow conflicts.
- **When tuning tech requirements:** run `sim_bench` with `epic5_research.json` (5 seeds x 10k ticks). Check `techs_unlocked` metric — it should show progression, not stuck at 1.
- **When functions approach 80 lines:** pre-plan helper extraction before adding new logic.

## Cross-references

- `docs/solutions/patterns/multi-epic-project-execution.md` — project workflow patterns
- `docs/solutions/integration-issues/module-behavior-extensibility.md` — module system extension patterns
- Memory: `project_propellant_balance_observations.md` — related balance tuning findings from Epic 4
