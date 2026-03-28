---
title: "Molten Materials: Thermal Container and Port/Link System"
category: patterns
date: 2026-03-28
tags: [sim_core, thermal, module-behavior, phase-transition, port-link, content-driven, multi-ticket]
components: [sim_core/thermal.rs, sim_core/types/content.rs, sim_core/types/state.rs, sim_core/commands.rs, sim_core/station/thermal.rs]
tickets: [VIO-201, VIO-217, VIO-218, VIO-223, VIO-224, VIO-225, VIO-226, VIO-227, VIO-229, VIO-230]
---

# Problem

Implement a molten material processing pipeline: phase transitions (solid/liquid), insulated containers (crucibles), directed material flow between modules via ports and links, and freeze detection during transfer. This required a new module behavior type, new command types, and integer-precision thermal arithmetic — all while maintaining backward compatibility and determinism.

# Key Patterns Discovered

## 1. Adding a New ModuleBehaviorDef Variant

Adding `ThermalContainer(ThermalContainerDef)` required changes in 5 locations:

1. `ModuleBehaviorDef` enum — add variant
2. `ModuleBehaviorDef::type_name()` — add match arm
3. `ModuleBehaviorDef::interval_ticks()` — add to passive (None) arm
4. `ModuleBehaviorDef::default_state()` — return new `ModuleKindState` + `BehaviorType`
5. `ModuleBehaviorDef::power_priority()` — add to passive arm

Also required:
- `ModuleKindState::ThermalContainer(ThermalContainerState)` variant
- `BehaviorType::ThermalContainer` variant
- Re-exports in `lib.rs`

**Key insight:** If the new variant has no tick interval (passive module like Storage/Equipment), it naturally falls into existing wildcard match arms. Only the `ModuleBehaviorDef` methods need explicit updates. The VIO-321 common-fields refactor means no match-arm explosion in station tick code.

## 2. Milli-Kelvin Unit Conversion Bug

The thermal container cooling calculation initially used milli-Kelvin directly with a W/K coefficient:

```rust
// BUG: temp_diff is in mK, but coefficient is W/K
let temp_diff = f64::from(props.temp_mk) - f64::from(sink_temp_mk);
let cooling_j = -(f64::from(cooling_coeff) * temp_diff * dt_s);
```

This produced cooling 1000x too strong. **Fix:** convert mK to K before applying Newton's law:

```rust
let temp_diff_k = (f64::from(props.temp_mk) - f64::from(sink_temp_mk)) / 1000.0;
let cooling_j = -(f64::from(cooling_coeff) * temp_diff_k * dt_s);
```

**Prevention:** The existing `heat_to_temp_delta_mk()` helper in `thermal.rs` handles this conversion correctly. Always use established helpers or explicitly comment the unit at each arithmetic step.

## 3. Phase Transition Cooling Direction Bug

The `update_phase()` function's liquid cooling path used `temp_distance_to_heat(current_temp, solidification_point)` — but that helper only computes *upward* distances (returns 0 when `to < from`). For cooling, the arguments must be swapped:

```rust
// BUG: returns 0 when liquid is above solidification point
let heat_to_solidify = temp_distance_to_heat(props.temp_mk, solidification_point, cap);

// FIX: swap args to compute downward distance
let heat_to_solidify = temp_distance_to_heat(solidification_point, props.temp_mk, cap);
```

**Prevention:** Helper functions that compute directional quantities should either (a) take an explicit direction parameter, or (b) be named to indicate directionality (e.g., `heat_to_raise_temp`).

## 4. Bulk Field Addition with Python Scripts

Adding `ports: Vec::new()` to 67 `ModuleDef` constructors and `thermal_links: Vec::new()` to 27 `StationState` constructors was done via Python regex scripts:

```python
pattern = re.compile(r'(required_tech: None,)\n(\s*)(},)')
new_text = pattern.sub(r'\1\n\2ports: Vec::new(),\n\2\3', text)
```

**Gotcha:** The script must distinguish between struct types that share field names. `required_tech: None,` appears in both `ModuleDef` and `RecipeDef` constructors. The distinguishing context is what follows: `ModuleDef` ends with `},` while `RecipeDef` has `tags:` after. Similarly, `leaders:` appears in both `StationState` and `ShipState` — the script accidentally added `thermal_links` to 7 `ShipState` constructors that had to be manually removed.

**Prevention:** Use the compile error as the source of truth — run `cargo check`, let missing-field errors identify exactly which constructors need updating, then use targeted scripts that match surrounding context (not just the field pattern).

## 5. Flaky Determinism Test from AHashMap Ordering

The `determinism_same_seed_identical_state_with_real_content` test intermittently fails because `module_defs` is an `AHashMap` (non-deterministic hash ordering). When the autopilot installs modules, the iteration order over `module_defs` can vary between runs, causing different module installation order and thus different wear values.

This is a pre-existing issue exposed by adding new module content (crucible, casting mold). The test passes ~60% of the time.

**Root cause:** `AHashMap` was chosen for hot-path performance but breaks determinism guarantees when iterated for autopilot decisions.

## 6. Port/Link Architecture: Thin Abstraction

The port/link system was intentionally kept minimal:
- **Ports** are static declarations on `ModuleDef` (direction + filter)
- **Links** are explicit `Vec<ThermalLink>` on `StationState` (no routing/solver)
- **Transfers** are discrete commands, not automatic flow

This "thin" design avoids premature complexity (no graph solver, no pathfinding, no flow rates). Material moves via explicit `TransferMolten` commands. The freeze detection check is a simple temperature comparison at transfer time.

# Cross-References

- `docs/solutions/patterns/crew-system-multi-ticket-implementation.md` — Same bulk-field-addition pattern
- `docs/solutions/logic-errors/deterministic-integer-arithmetic.md` — Integer thermal arithmetic
- `docs/solutions/integration-issues/module-behavior-extensibility.md` — Adding module variants
