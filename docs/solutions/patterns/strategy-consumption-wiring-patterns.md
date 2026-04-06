---
title: "Strategy Consumption Wiring — Threshold Migration + Weighted Priority Halving"
category: patterns
date: 2026-04-06
tags: [strategy-config, autopilot, priority-halving, threshold-migration, phase-switching, sim_control]
problem_type: "architecture-decision"
component: "sim_control, sim_core"
severity: medium
related_tickets: [VIO-481, VIO-607, VIO-608, VIO-609, VIO-606]
---

# Strategy Consumption Wiring — P6 Phase C3

## Problem

Phase C (VIO-479/480) built the strategic layer foundation: `StrategyConfig` types, the rule interpreter, cache gating. But no consumer actually read the output. Phase C3 wires the output into every autopilot agent, migrating 13 hardcoded thresholds and replacing a fixed task-priority list with weighted dynamic selection.

## Solution

### 1. Threshold Migration: Content IDs vs Tunable Thresholds

The key scoping decision: `AutopilotConfig` has two categories of fields.

**Content identifiers stay on `content.autopilot`** (not tunable at runtime):
- `propellant_role`, `propellant_support_role`, `shipyard_role` (module role IDs)
- `volatile_element`, `propellant_element`, `primary_mining_element` (element IDs)
- `deep_scan_tech`, `ship_construction_tech` (tech IDs)
- `export_component`, `export_elements`, `deep_scan_targets` (structured content)
- `task_priority` (replaced by weighted selection, but the content list is still loaded)

**Numeric thresholds migrate to `state.strategy_config.*`** (tunable at runtime):
- `lh2_threshold_kg`, `lh2_abundant_multiplier`, `volatile_threshold_kg`
- `refinery_threshold_kg`, `slag_jettison_pct`, `export_batch_size_kg`, `export_min_revenue`
- `budget_cap_fraction`, `refuel_threshold_pct`, `refuel_max_pct`
- `power_deficit_threshold_kw`, `shipyard_component_count`, `crew_hire_projection_minutes`

**Behavioral equivalence assertion**: The regression test asserts all 13 `StrategyConfig` default values match `AutopilotConfig` values field-by-field, catching any future drift.

### 2. Weighted Priority Halving (DFHack Labormanager Pattern)

Replaced the fixed `content.autopilot.task_priority` list (`["Mine", "Survey", "DeepScan"]`) with dynamic weighted selection driven by `ConcernPriorities`.

```rust
// Running weights from ConcernPriorities
let mut weights = [
    ("Mine", priorities.mining),
    ("Survey", priorities.survey),
    ("DeepScan", priorities.deep_scan),
];

for ship_id in assignable {
    // Sort by weight descending each iteration
    weights.sort_by(|a, b| b.1.total_cmp(&a.1));
    for &mut (priority, ref mut weight) in &mut weights {
        if *weight <= 0.0 { break; }
        if let Some(obj) = try_assign(priority) {
            agent.objective = Some(obj);
            *weight *= 0.5; // Halve after assignment
            break;
        }
    }
}
```

**Why halving works**: With mining=0.7 and survey=0.6, the first ship mines (0.7 > 0.6). Mining halves to 0.35, so the second ship surveys (0.6 > 0.35). The third ship mines again if mining recovers enough candidates. This naturally distributes ships proportionally to weights without a fixed ordering.

**Edge case**: Zero weights skip assignment entirely (`break` on `<= 0.0`), producing the same effect as the old empty `task_priority` list.

### 3. Phase-Driven Auto-Switching

`GamePhase` (already tracked by the progression system) maps to a default `StrategyMode`:
- Startup/Orbital → Balanced
- Industrial/Expansion/DeepSpace → Expand

The controller detects phase transitions via `StrategyRuntimeState.last_phase` and emits a `SetStrategyConfig` command to update the mode (plus per-phase priority presets from `content/strategy_phase_presets.json`). This goes through the command pipeline like any other state mutation, keeping `GameState` consistent.

**Manual override**: `strategy_config.mode_override: Option<StrategyMode>` disables auto-switching when set.

**1-tick lag**: Strategy is evaluated with the old mode before the switch command is emitted. The switch takes effect on the next tick when `apply_commands` processes it. This is intentional — commands are applied in `tick()`, not in `generate_commands`.

### 4. Budget Scaling by Priority Weight

Ground facility sensor spending scales with the `research` priority weight:
```rust
let research_scale = 0.5 + 0.5 * f64::from(priorities.research);
let effective_budget = base_budget * research_scale;
```

Formula range: research=0.0 → 50% budget, research=1.0 → 100% budget. Never zero (agents always have some spending authority), but phase presets naturally adjust: Startup (research=0.9) spends aggressively on sensors, Industrial (research=0.5) is conservative.

## Prevention

- **Behavioral equivalence test**: When migrating threshold sources, assert all migrated fields match their original defaults in the regression test. The test at `lib.rs:baseline_autopilot_config_regression` checks all 13 fields.
- **Integration tests for causal links**: VIO-606 added 4 tests across 10 seeds proving mining-focused → more revenue, research-focused → more techs, fleet_target=5 → more ships, Expand → more exports. These catch regressions where config changes stop affecting outcomes.
- **`too_many_lines` extraction**: When adding phase-switching logic to `generate_commands`, extract to a helper method immediately rather than fixing after CI catches it.
