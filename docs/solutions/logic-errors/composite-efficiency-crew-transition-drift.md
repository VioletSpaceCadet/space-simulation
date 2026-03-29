---
title: "Reverse-engineering factors from composite values causes drift bugs"
category: logic-errors
date: 2026-03-28
tags: [efficiency, crew, wear, transition-detection, state-management]
components: [sim_core, station/mod.rs]
ticket: VIO-438
---

## Problem

After adding a combined `efficiency: f32` field to `ModuleState` (product of power, crew, and wear factors), crew satisfaction transition events (`ModuleUnderstaffed`/`ModuleFullyStaffed`) fired spuriously when wear crossed band boundaries — even though crew assignments hadn't changed.

## Root Cause

The `update_crew_satisfaction` function needed to detect whether crew satisfaction had changed since the last tick. The initial implementation tried to derive the previous crew factor by dividing the stored composite efficiency by the current wear and power factors:

```rust
let prev_crew_factor = if wear_factor > 0.0 && power_factor > 0.0 {
    module.efficiency / (wear_factor * power_factor)
} else {
    1.0 // can't determine; assume satisfied
};
```

This is incorrect because **wear changes between when efficiency is stored and when it's read**:

1. **Tick N, step 3**: `efficiency = power_N * crew_N * wear_factor_N` stored
2. **Tick N, steps 4-6**: Processors add wear, maintenance repairs wear, thermal overheat bumps wear → `wear` is now different
3. **Tick N+1, step 1**: `update_crew_satisfaction` reads `module.efficiency` (computed with `wear_factor_N`) but derives `prev_crew_factor` using `wear_factor(wear_N')` (the current, post-mutation value)

When wear crosses a band boundary (e.g., nominal 1.0 → degraded 0.75), the derived `prev_crew_factor` becomes `crew_N * 1.0 / 0.75 = crew_N * 1.333`, falsely detecting a satisfaction transition.

## Solution

Store the previous satisfaction state directly instead of reverse-engineering it from the composite value:

```rust
// On ModuleState:
#[serde(skip, default = "default_prev_crew_satisfied")]
pub prev_crew_satisfied: bool,

// In update_crew_satisfaction:
let now_satisfied = is_crew_satisfied(&module.assigned_crew, &def.crew_requirement);
if now_satisfied != module.prev_crew_satisfied {
    transitions.push((module.id.clone(), module.prev_crew_satisfied, now_satisfied));
}
```

Initialize `prev_crew_satisfied` alongside efficiency in `init_module_efficiency()` and `handle_install_module()`.

## Prevention

**Never reverse-engineer individual factors from a composite value when intermediate mutations can change any factor.** If you need to detect transitions in one factor of a product, store that factor (or its derived boolean) separately. The cost of one extra `bool` field is negligible compared to debugging spurious event storms.

This applies broadly: any `composite = A * B * C` where you later need to know "did A change?" requires storing A independently — you cannot reliably recover A from `composite / (B_current * C_current)` if B or C mutated between store and read.
