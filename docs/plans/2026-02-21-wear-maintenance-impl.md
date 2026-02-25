# Wear + Maintenance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add wear accumulation, efficiency bands, auto-disable, and a Maintenance Bay module to station modules.

**Architecture:** `WearState` struct embedded in `ModuleState`, `wear_efficiency()` pure function in a new `sim_core::wear` module. `MaintenanceDef`/`MaintenanceState` as new variants in existing enums. RepairKit uses existing `InventoryItem::Component`. Wear math is generic for future ship reuse.

**Tech Stack:** Rust (sim_core, sim_control, sim_world), JSON content files.

---

### Task 1: Add wear constants to types and content

**Files:**
- Modify: `crates/sim_core/src/types.rs:547-574` (Constants struct)
- Modify: `content/constants.json`
- Modify: `crates/sim_core/src/test_fixtures.rs:76-99` (base_content Constants)
- Modify: `crates/sim_core/src/test_fixtures.rs:120-143` (minimal_content Constants)

**Step 1: Write the failing test**

In `crates/sim_core/src/types.rs`, the Constants struct doesn't have wear fields yet. Add a test in `crates/sim_core/src/tests.rs` that verifies constants have wear fields:

```rust
// At bottom of crates/sim_core/src/tests.rs
#[test]
fn test_constants_have_wear_fields() {
    let content = test_content();
    assert!((content.constants.wear_band_degraded_threshold - 0.5).abs() < 1e-5);
    assert!((content.constants.wear_band_critical_threshold - 0.8).abs() < 1e-5);
    assert!((content.constants.wear_band_degraded_efficiency - 0.75).abs() < 1e-5);
    assert!((content.constants.wear_band_critical_efficiency - 0.5).abs() < 1e-5);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_constants_have_wear_fields`
Expected: FAIL — fields don't exist yet.

**Step 3: Add wear fields to Constants struct**

In `crates/sim_core/src/types.rs`, add after `autopilot_refinery_threshold_kg`:

```rust
    // Wear system
    pub wear_band_degraded_threshold: f32,
    pub wear_band_critical_threshold: f32,
    pub wear_band_degraded_efficiency: f32,
    pub wear_band_critical_efficiency: f32,
```

Update `content/constants.json` — add after `"autopilot_refinery_threshold_kg": 500.0`:

```json
  "wear_band_degraded_threshold": 0.5,
  "wear_band_critical_threshold": 0.8,
  "wear_band_degraded_efficiency": 0.75,
  "wear_band_critical_efficiency": 0.5
```

Update `crates/sim_core/src/test_fixtures.rs` `base_content()` Constants — add after `autopilot_refinery_threshold_kg: 500.0,`:

```rust
                wear_band_degraded_threshold: 0.5,
                wear_band_critical_threshold: 0.8,
                wear_band_degraded_efficiency: 0.75,
                wear_band_critical_efficiency: 0.5,
```

Same for `minimal_content()`.

Also update the inline Constants in `crates/sim_core/src/engine.rs` test `replenish_test_content()` (around line 373-396).

**Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: ALL PASS (including the new test).

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(wear): add wear band constants to types and content"
```

---

### Task 2: Add WearState, wear_efficiency(), and wear module

**Files:**
- Create: `crates/sim_core/src/wear.rs`
- Modify: `crates/sim_core/src/lib.rs` (add `pub mod wear;`)
- Modify: `crates/sim_core/src/types.rs` (add WearState struct)

**Step 1: Write the failing tests**

Create `crates/sim_core/src/wear.rs` with tests first:

```rust
//! Wear math — generic across modules and (future) ships.

use crate::Constants;

/// Standalone wear state, embedded wherever wear applies.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WearState {
    pub wear: f32,
}

impl Default for WearState {
    fn default() -> Self {
        Self { wear: 0.0 }
    }
}

/// Returns the efficiency multiplier for the given wear level.
/// Pure function — no mutation.
pub fn wear_efficiency(wear: f32, constants: &Constants) -> f32 {
    if wear >= constants.wear_band_critical_threshold {
        constants.wear_band_critical_efficiency
    } else if wear >= constants.wear_band_degraded_threshold {
        constants.wear_band_degraded_efficiency
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::base_content;

    #[test]
    fn nominal_band_full_efficiency() {
        let c = &base_content().constants;
        assert!((wear_efficiency(0.0, c) - 1.0).abs() < 1e-5);
        assert!((wear_efficiency(0.49, c) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn degraded_band_reduced_efficiency() {
        let c = &base_content().constants;
        assert!((wear_efficiency(0.5, c) - 0.75).abs() < 1e-5);
        assert!((wear_efficiency(0.79, c) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn critical_band_heavily_reduced() {
        let c = &base_content().constants;
        assert!((wear_efficiency(0.8, c) - 0.5).abs() < 1e-5);
        assert!((wear_efficiency(1.0, c) - 0.5).abs() < 1e-5);
    }
}
```

**Step 2: Register the module**

In `crates/sim_core/src/lib.rs`, add `pub mod wear;` after `mod types;`.
Add `pub use wear::{WearState, wear_efficiency};` to exports.

**Step 3: Run tests**

Run: `cargo test -p sim_core wear`
Expected: ALL PASS.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(wear): add WearState struct and wear_efficiency() pure function"
```

---

### Task 3: Add wear to ModuleState, new behavior/state variants, new events

**Files:**
- Modify: `crates/sim_core/src/types.rs:147-164` (ModuleState, ModuleKindState, add MaintenanceState)
- Modify: `crates/sim_core/src/types.rs:459-473` (ModuleDef, ModuleBehaviorDef, add MaintenanceDef, wear_per_run)
- Modify: `crates/sim_core/src/types.rs:304-397` (Event enum — add new variants)

**Step 1: Add wear to ModuleState**

In `types.rs`, modify `ModuleState` (line 147):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleState {
    pub id: ModuleInstanceId,
    pub def_id: String,
    pub enabled: bool,
    pub kind_state: ModuleKindState,
    pub wear: WearState,
}
```

Add import `use crate::wear::WearState;` at top of types.rs (or since types is the root, put WearState in types.rs directly and re-export from wear.rs... Actually WearState is already in wear.rs, so import from there).

Wait — `types.rs` is used by everything and `wear.rs` imports from `types.rs`. To avoid circular deps, define `WearState` in `types.rs` and have `wear.rs` use it from there.

Actually, looking at the codebase pattern: `types.rs` defines all structs, other modules import from types. So:
- Define `WearState` in `types.rs`
- `wear.rs` imports `WearState` and `Constants` from crate (which re-exports from types)
- This avoids any circular dependency

Move `WearState` to `types.rs`:

```rust
/// Standalone wear state, embedded wherever wear applies.
/// Generic — used by station modules now, ships later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WearState {
    pub wear: f32,
}

impl Default for WearState {
    fn default() -> Self {
        Self { wear: 0.0 }
    }
}
```

And `wear.rs` just has `wear_efficiency()` and tests.

**Step 2: Add new ModuleKindState and ModuleBehaviorDef variants**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
    Maintenance(MaintenanceState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceState {
    pub ticks_since_last_run: u64,
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
    Maintenance(MaintenanceDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceDef {
    pub repair_interval_ticks: u64,
    pub wear_reduction_per_run: f32,
    pub repair_kit_cost: u32,
}
```

Add `wear_per_run` to `ModuleDef`:

```rust
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
    pub power_consumption_per_run: f32,
    pub wear_per_run: f32,
    pub behavior: ModuleBehaviorDef,
}
```

**Step 3: Add new Event variants**

In the `Event` enum, add:

```rust
    WearAccumulated {
        station_id: StationId,
        module_id: ModuleInstanceId,
        wear_before: f32,
        wear_after: f32,
    },
    ModuleAutoDisabled {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    MaintenanceRan {
        station_id: StationId,
        target_module_id: ModuleInstanceId,
        wear_before: f32,
        wear_after: f32,
        repair_kits_remaining: u32,
    },
```

**Step 4: Fix all compilation errors**

Every place that constructs `ModuleState` now needs `wear: WearState::default()`. Every place that constructs `ModuleDef` needs `wear_per_run: 0.0` (or appropriate value). Every match on `ModuleKindState` or `ModuleBehaviorDef` needs the new arm.

Key locations to update:
- `crates/sim_core/src/engine.rs:100-120` — `InstallModule` command creates `ModuleState` — add `wear: WearState::default()`, handle `Maintenance` variant for kind_state
- `crates/sim_core/src/station.rs:46-49` — match on `ModuleBehaviorDef` — add `Maintenance` arm
- `crates/sim_core/src/metrics.rs:159-176` — match on `ModuleBehaviorDef` — add `Maintenance` arm
- `crates/sim_core/src/tests.rs:1278-1286` — `state_with_refinery()` creates ModuleState — add `wear`
- `crates/sim_core/src/tests.rs:1236-1270` — `refinery_content()` creates ModuleDef — add `wear_per_run`
- `crates/sim_control/src/lib.rs:94` — match on `ModuleKindState` — add `Maintenance` arm
- `crates/sim_control/src/lib.rs:455-463` — tests create ModuleState — add `wear`
- `crates/sim_world/src/lib.rs:84-85` — content validation match on `ModuleBehaviorDef` — add arm
- `content/module_defs.json` — add `"wear_per_run": 0.01` to existing refinery def
- All test ModuleDef constructions need `wear_per_run`

**Step 5: Run tests**

Run: `cargo test`
Expected: ALL PASS — no behavior change yet, just structural additions.

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(wear): add WearState to ModuleState, Maintenance variants, new events"
```

---

### Task 4: Apply wear efficiency to processor output and accumulate wear

**Files:**
- Modify: `crates/sim_core/src/station.rs:118-287` (resolve_processor_run — apply efficiency multiplier, increment wear, emit events)

**Step 1: Write the failing test**

Add to `crates/sim_core/src/tests.rs`:

```rust
#[test]
fn test_refinery_output_reduced_by_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set wear to degraded band (0.6 → 75% efficiency)
    state.stations.get_mut(&station_id).unwrap().modules[0].wear.wear = 0.6;

    let mut rng = make_rng();
    // Tick twice to reach processing_interval_ticks=2
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let material_kg = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Material { element, kg, .. } = i {
            if element == "Fe" { Some(*kg) } else { None }
        } else { None }
    }).unwrap_or(0.0);

    // Without wear: 500 kg input × 0.7 Fe fraction = 350 kg
    // With 75% efficiency: 350 × 0.75 = 262.5 kg
    assert!(
        (material_kg - 262.5).abs() < 1.0,
        "degraded module should produce ~262.5 kg Fe, got {material_kg}"
    );
}

#[test]
fn test_refinery_accumulates_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let mut rng = make_rng();

    // Tick twice to trigger refinery run
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let wear = state.stations[&station_id].modules[0].wear.wear;
    let expected_wear = content.module_defs.iter()
        .find(|d| d.id == "module_basic_iron_refinery")
        .unwrap()
        .wear_per_run;
    assert!(
        (wear - expected_wear).abs() < 1e-5,
        "wear should be {expected_wear} after one run, got {wear}"
    );
}

#[test]
fn test_refinery_auto_disables_at_max_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set wear just below 1.0 so next run pushes it over
    state.stations.get_mut(&station_id).unwrap().modules[0].wear.wear = 0.995;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(!station.modules[0].enabled, "module should be auto-disabled at wear >= 1.0");
    assert!(
        events.iter().any(|e| matches!(e.event, Event::ModuleAutoDisabled { .. })),
        "ModuleAutoDisabled event should be emitted"
    );
}

#[test]
fn test_wear_accumulated_event_emitted() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events.iter().any(|e| matches!(e.event, Event::WearAccumulated { .. })),
        "WearAccumulated event should be emitted when refinery runs"
    );
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core test_refinery_output_reduced_by_wear test_refinery_accumulates_wear test_refinery_auto_disables test_wear_accumulated`
Expected: FAIL — wear not applied yet.

**Step 3: Implement wear in resolve_processor_run**

In `crates/sim_core/src/station.rs`, modify `resolve_processor_run()`:

After computing `material_kg` (around line 209), apply wear efficiency:

```rust
// Apply wear efficiency to material output
let wear_value = state.stations.get(station_id)
    .map(|s| s.modules[module_idx].wear.wear)
    .unwrap_or(0.0);
let efficiency = crate::wear::wear_efficiency(wear_value, &content.constants);
material_kg *= efficiency;
```

Also apply to slag (slag_kg calculation around line 236):
```rust
slag_kg *= efficiency;
```

After the RefineryRan event emission (around line 286), add wear accumulation:

```rust
// Accumulate wear
let wear_per_run = content.module_defs.iter()
    .find(|d| d.id == def_id)
    .map(|d| d.wear_per_run)
    .unwrap_or(0.0);

if wear_per_run > 0.0 {
    if let Some(station) = state.stations.get_mut(station_id) {
        let module = &mut station.modules[module_idx];
        let wear_before = module.wear.wear;
        module.wear.wear = (module.wear.wear + wear_per_run).min(1.0);
        let wear_after = module.wear.wear;

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::WearAccumulated {
                station_id: station_id.clone(),
                module_id: module.id.clone(),
                wear_before,
                wear_after,
            },
        ));

        // Auto-disable if wear hit 1.0
        if module.wear.wear >= 1.0 {
            let mid = module.id.clone();
            module.enabled = false;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleAutoDisabled {
                    station_id: station_id.clone(),
                    module_id: mid,
                },
            ));
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: ALL PASS.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(wear): apply wear efficiency to processor output, accumulate wear, auto-disable"
```

---

### Task 5: Add Maintenance Bay tick logic

**Files:**
- Modify: `crates/sim_core/src/station.rs` (add maintenance tick after processor tick)
- Create: `content/component_defs.json`

**Step 1: Write the failing tests**

Add to `crates/sim_core/src/tests.rs`:

```rust
fn maintenance_content() -> GameContent {
    let mut content = refinery_content();
    content.module_defs.push(ModuleDef {
        id: "module_maintenance_bay".to_string(),
        name: "Maintenance Bay".to_string(),
        mass_kg: 2000.0,
        volume_m3: 5.0,
        power_consumption_per_run: 5.0,
        wear_per_run: 0.0,
        behavior: ModuleBehaviorDef::Maintenance(MaintenanceDef {
            repair_interval_ticks: 2,
            wear_reduction_per_run: 0.2,
            repair_kit_cost: 1,
        }),
    });
    content
}

fn state_with_maintenance(content: &GameContent) -> GameState {
    let mut state = state_with_refinery(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Add maintenance bay module
    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0002".to_string()),
        def_id: "module_maintenance_bay".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Maintenance(MaintenanceState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
    });

    // Add repair kits
    station.inventory.push(InventoryItem::Component {
        component_id: ComponentId("repair_kit".to_string()),
        count: 5,
        quality: 1.0,
    });

    state
}

#[test]
fn test_maintenance_repairs_most_worn_module() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set refinery to worn state
    state.stations.get_mut(&station_id).unwrap().modules[0].wear.wear = 0.6;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        (station.modules[0].wear.wear - 0.4).abs() < 0.1,
        "wear should be reduced by ~0.2, got {}",
        station.modules[0].wear.wear
    );
    assert!(
        events.iter().any(|e| matches!(e.event, Event::MaintenanceRan { .. })),
        "MaintenanceRan event should be emitted"
    );
}

#[test]
fn test_maintenance_consumes_repair_kit() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0].wear.wear = 0.6;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let kits = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Component { component_id, count, .. } = i {
            if component_id.0 == "repair_kit" { Some(*count) } else { None }
        } else { None }
    }).unwrap_or(0);
    assert_eq!(kits, 4, "one repair kit should be consumed, got {kits} remaining");
}

#[test]
fn test_maintenance_skips_when_no_repair_kits() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0].wear.wear = 0.6;
    // Remove all repair kits
    state.stations.get_mut(&station_id).unwrap().inventory
        .retain(|i| !matches!(i, InventoryItem::Component { component_id, .. } if component_id.0 == "repair_kit"));

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    // Wear should increase (refinery ran) but not decrease (no kits for maintenance)
    assert!(
        station.modules[0].wear.wear > 0.6,
        "wear should not decrease without repair kits"
    );
}

#[test]
fn test_maintenance_skips_when_no_worn_modules() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // All modules at wear 0.0 — remove ore so refinery doesn't run and accumulate wear
    state.stations.get_mut(&station_id).unwrap().inventory
        .retain(|i| !matches!(i, InventoryItem::Ore { .. }));

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Kits should not be consumed
    let station = &state.stations[&station_id];
    let kits = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Component { component_id, count, .. } = i {
            if component_id.0 == "repair_kit" { Some(*count) } else { None }
        } else { None }
    }).unwrap_or(0);
    assert_eq!(kits, 5, "no kits should be consumed when nothing is worn");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core test_maintenance_`
Expected: FAIL — maintenance tick logic doesn't exist.

**Step 3: Implement maintenance tick logic**

In `crates/sim_core/src/station.rs`, add a new function and call it from `tick_station_modules`:

After the processor module loop in `tick_station_modules`, add a second pass for maintenance modules:

```rust
fn tick_maintenance_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let module_count = state.stations.get(station_id).map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let (def_id, interval, power_needed, repair_reduction, kit_cost) = {
            let Some(station) = state.stations.get(station_id) else { return };
            let module = &station.modules[module_idx];
            if !module.enabled { continue; }
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else { continue };
            let ModuleBehaviorDef::Maintenance(maint_def) = &def.behavior else { continue };
            (
                module.def_id.clone(),
                maint_def.repair_interval_ticks,
                def.power_consumption_per_run,
                maint_def.wear_reduction_per_run,
                maint_def.repair_kit_cost,
            )
        };

        // Tick timer
        {
            let Some(station) = state.stations.get_mut(station_id) else { return };
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run += 1;
                if ms.ticks_since_last_run < interval { continue; }
            } else { continue; }
        }

        // Check power
        {
            let Some(station) = state.stations.get(station_id) else { return };
            if station.power_available_per_tick < power_needed { continue; }
        }

        // Find most worn module (not self, wear > 0.0)
        let target = {
            let Some(station) = state.stations.get(station_id) else { return };
            let self_id = &station.modules[module_idx].id;
            let mut candidates: Vec<(usize, f32, &ModuleInstanceId)> = station.modules.iter()
                .enumerate()
                .filter(|(_, m)| &m.id != self_id && m.wear.wear > 0.0)
                .map(|(idx, m)| (idx, m.wear.wear, &m.id))
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.2.0.cmp(&b.2.0)));
            candidates.first().map(|(idx, _, _)| *idx)
        };

        let Some(target_idx) = target else {
            // Nothing worn — reset timer but don't consume kit
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        };

        // Consume repair kit
        let has_kit = {
            let Some(station) = state.stations.get_mut(station_id) else { return };
            let kit_slot = station.inventory.iter_mut().find(|i| {
                matches!(i, InventoryItem::Component { component_id, count, .. }
                    if component_id.0 == "repair_kit" && *count >= kit_cost)
            });
            if let Some(InventoryItem::Component { count, .. }) = kit_slot {
                *count -= kit_cost;
                true
            } else {
                false
            }
        };

        if !has_kit {
            // Reset timer even if no kit — don't let it fire every tick once interval passes
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Remove empty component stacks
        if let Some(station) = state.stations.get_mut(station_id) {
            station.inventory.retain(|i| {
                !matches!(i, InventoryItem::Component { count, .. } if *count == 0)
            });
        }

        // Apply repair
        let (target_module_id, wear_before, wear_after, kits_remaining) = {
            let Some(station) = state.stations.get_mut(station_id) else { return };
            let target_module = &mut station.modules[target_idx];
            let wear_before = target_module.wear.wear;
            target_module.wear.wear = (target_module.wear.wear - repair_reduction).max(0.0);
            let wear_after = target_module.wear.wear;
            let target_module_id = target_module.id.clone();

            let kits_remaining: u32 = station.inventory.iter()
                .filter_map(|i| {
                    if let InventoryItem::Component { component_id, count, .. } = i {
                        if component_id.0 == "repair_kit" { Some(*count) } else { None }
                    } else { None }
                })
                .sum();

            // Reset timer
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run = 0;
            }

            (target_module_id, wear_before, wear_after, kits_remaining)
        };

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::MaintenanceRan {
                station_id: station_id.clone(),
                target_module_id,
                wear_before,
                wear_after,
                repair_kits_remaining: kits_remaining,
            },
        ));
    }
}
```

Call `tick_maintenance_modules` from `tick_stations` after the processor loop:

In `tick_stations`:
```rust
pub(crate) fn tick_stations(...) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in station_ids {
        tick_station_modules(state, &station_id, content, events);
        tick_maintenance_modules(state, &station_id, content, events);
    }
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: ALL PASS.

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(wear): add Maintenance Bay tick logic — repair most worn, consume RepairKit"
```

---

### Task 6: Add wear metrics

**Files:**
- Modify: `crates/sim_core/src/metrics.rs:17-61` (MetricsSnapshot — add wear fields)
- Modify: `crates/sim_core/src/metrics.rs:127-299` (compute_metrics — compute wear metrics)
- Modify: `crates/sim_core/src/metrics.rs:302-353` (CSV header and row — add columns)

**Step 1: Write the failing test**

Add to metrics tests (in `crates/sim_core/src/metrics.rs`):

```rust
#[test]
fn test_wear_metrics() {
    let mut content = empty_content();
    content.module_defs = vec![crate::ModuleDef {
        id: "module_basic_iron_refinery".to_string(),
        name: "Basic Iron Refinery".to_string(),
        mass_kg: 5000.0,
        volume_m3: 10.0,
        power_consumption_per_run: 10.0,
        wear_per_run: 0.01,
        behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
            processing_interval_ticks: 60,
            recipes: vec![],
        }),
    }];

    let mut state = empty_state();
    let station = make_station(
        vec![InventoryItem::Component {
            component_id: crate::ComponentId("repair_kit".to_string()),
            count: 3,
            quality: 1.0,
        }],
        vec![
            ModuleState {
                id: ModuleInstanceId("mod_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: true,
                kind_state: ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                }),
                wear: crate::WearState { wear: 0.3 },
            },
            ModuleState {
                id: ModuleInstanceId("mod_0002".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: true,
                kind_state: ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                }),
                wear: crate::WearState { wear: 0.7 },
            },
        ],
    );
    state.stations.insert(station.id.clone(), station);

    let snapshot = compute_metrics(&state, &content);
    assert!((snapshot.avg_module_wear - 0.5).abs() < 1e-5, "avg wear should be 0.5");
    assert!((snapshot.max_module_wear - 0.7).abs() < 1e-5, "max wear should be 0.7");
    assert_eq!(snapshot.repair_kits_remaining, 3);
}
```

**Step 2: Implement wear metrics**

Add fields to `MetricsSnapshot`:

```rust
    // Wear
    pub avg_module_wear: f32,
    pub max_module_wear: f32,
    pub repair_kits_remaining: u32,
```

In `compute_metrics`, add wear accumulation in the station loop:

```rust
    let mut wear_sum = 0.0_f32;
    let mut wear_count = 0_u32;
    let mut max_wear = 0.0_f32;
    let mut total_repair_kits = 0_u32;
```

Inside station loop, after module iteration:
```rust
        for module in &station.modules {
            // existing refinery checks...
            // Add wear tracking for all enabled processors
            if module.enabled {
                let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else { continue };
                if matches!(def.behavior, ModuleBehaviorDef::Processor(_)) {
                    wear_sum += module.wear.wear;
                    wear_count += 1;
                    if module.wear.wear > max_wear {
                        max_wear = module.wear.wear;
                    }
                }
            }
        }

        // Count repair kits
        for item in &station.inventory {
            if let InventoryItem::Component { component_id, count, .. } = item {
                if component_id.0 == "repair_kit" {
                    total_repair_kits += *count;
                }
            }
        }
```

Compute averages:
```rust
    let avg_module_wear = if wear_count > 0 { wear_sum / wear_count as f32 } else { 0.0 };
```

Add to MetricsSnapshot construction and CSV header/row.

Update `METRICS_VERSION` to 2.

**Step 3: Run tests**

Run: `cargo test`
Expected: ALL PASS.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(wear): add wear metrics — avg_module_wear, max_module_wear, repair_kits_remaining"
```

---

### Task 7: Add Maintenance Bay module def and RepairKits to content/world gen

**Files:**
- Modify: `content/module_defs.json` — add maintenance bay module def, add `wear_per_run` to refinery
- Create: `content/component_defs.json` — define repair_kit
- Modify: `crates/sim_core/src/types.rs` — add `ComponentDef` and `component_defs` to `GameContent`
- Modify: `crates/sim_world/src/lib.rs` — load component_defs, add RepairKits + Maintenance Bay to starting inventory
- Modify: `crates/sim_world/src/lib.rs:35-143` — validate component refs in maintenance defs

**Step 1: Add ComponentDef to types**

In `types.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
}
```

Add to `GameContent`:
```rust
pub struct GameContent {
    // ...existing fields...
    pub component_defs: Vec<ComponentDef>,
}
```

**Step 2: Create content/component_defs.json**

```json
[
  {
    "id": "repair_kit",
    "name": "Repair Kit",
    "mass_kg": 50.0,
    "volume_m3": 0.1
  }
]
```

**Step 3: Add maintenance bay to module_defs.json**

```json
  {
    "id": "module_maintenance_bay",
    "name": "Maintenance Bay",
    "mass_kg": 2000.0,
    "volume_m3": 5.0,
    "power_consumption_per_run": 5.0,
    "wear_per_run": 0.0,
    "behavior": {
      "Maintenance": {
        "repair_interval_ticks": 30,
        "wear_reduction_per_run": 0.2,
        "repair_kit_cost": 1
      }
    }
  }
```

Add `"wear_per_run": 0.01` to existing refinery def.

**Step 4: Update sim_world content loading**

In `sim_world/src/lib.rs`, load `component_defs.json`:

```rust
    let component_defs: Vec<ComponentDef> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("component_defs.json"))
            .context("reading component_defs.json")?,
    )
    .context("parsing component_defs.json")?;
```

Add `component_defs` to `GameContent` construction.

**Step 5: Add RepairKits and Maintenance Bay to starting inventory**

In `build_initial_state`, add to station inventory:

```rust
        inventory: vec![
            InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 5,
                quality: 1.0,
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_maint_0001".to_string()),
                module_def_id: "module_maintenance_bay".to_string(),
            },
        ],
```

**Step 6: Update test fixtures**

Add `component_defs: vec![]` to `base_content()` and `minimal_content()` in test_fixtures.rs. Also update all inline `GameContent` constructions in tests (engine.rs replenish tests, sim_world tests).

**Step 7: Run tests**

Run: `cargo test`
Expected: ALL PASS.

**Step 8: Commit**

```bash
git add -A && git commit -m "feat(wear): add Maintenance Bay module def, RepairKit component, starting inventory"
```

---

### Task 8: Update autopilot for Maintenance Bay

**Files:**
- Modify: `crates/sim_control/src/lib.rs:60-111` (station_module_commands — handle Maintenance modules)

**Step 1: Write the failing test**

Add to `crates/sim_control/src/lib.rs` tests:

```rust
#[test]
fn test_autopilot_installs_maintenance_bay() {
    let mut content = autopilot_content();
    content.module_defs.push(sim_core::ModuleDef {
        id: "module_maintenance_bay".to_string(),
        name: "Maintenance Bay".to_string(),
        mass_kg: 2000.0,
        volume_m3: 5.0,
        power_consumption_per_run: 5.0,
        wear_per_run: 0.0,
        behavior: sim_core::ModuleBehaviorDef::Maintenance(sim_core::MaintenanceDef {
            repair_interval_ticks: 30,
            wear_reduction_per_run: 0.2,
            repair_kit_cost: 1,
        }),
    });
    let mut state = autopilot_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().inventory.push(
        sim_core::InventoryItem::Module {
            item_id: sim_core::ModuleItemId("module_item_maint".to_string()),
            module_def_id: "module_maintenance_bay".to_string(),
        },
    );

    let mut autopilot = AutopilotController;
    let mut next_id = 0u64;
    let commands = autopilot.generate_commands(&state, &content, &mut next_id);

    assert!(
        commands.iter().any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
        "autopilot should install Maintenance Bay module"
    );
}
```

**Step 2: Verify test passes**

The existing autopilot logic already auto-installs any `InventoryItem::Module` and auto-enables disabled modules. The only change needed is ensuring the `ModuleKindState` match in `station_module_commands` doesn't skip Maintenance modules when checking threshold. Currently it only sets threshold for `Processor` — that's correct, Maintenance modules don't need threshold.

But we need to make sure `InstallModule` command handler in engine.rs creates the right `ModuleKindState::Maintenance(MaintenanceState { ticks_since_last_run: 0 })` for maintenance modules. This was already handled in Task 3.

Run: `cargo test -p sim_control`
Expected: PASS (existing logic handles it).

**Step 3: Commit (if any changes needed)**

```bash
git add -A && git commit -m "test(wear): verify autopilot handles Maintenance Bay installation"
```

---

### Task 9: Update CLAUDE.md and reference docs

**Files:**
- Modify: `CLAUDE.md` — update public API list, module list
- Modify: `docs/reference.md` — add wear system, maintenance bay, RepairKit, new events, new metrics

**Step 1: Update CLAUDE.md**

Add `wear` to sim_core modules list. Add `wear_efficiency` to public API. Update content file count (now 8 files with component_defs.json). Add `MaintenanceDef`/`MaintenanceState`/`WearState` to types mentions. Add new events.

**Step 2: Update docs/reference.md**

Add sections for:
- WearState and wear_efficiency
- Maintenance Bay module behavior
- RepairKit component
- New events (WearAccumulated, ModuleAutoDisabled, MaintenanceRan)
- New metrics fields

**Step 3: Commit**

```bash
git add -A && git commit -m "docs: update CLAUDE.md and reference.md for wear + maintenance system"
```

---

### Task 10: Integration test — wear + maintenance full cycle

**Files:**
- Modify: `crates/sim_core/src/tests.rs` — add integration test

**Step 1: Write the test**

```rust
#[test]
fn test_wear_maintenance_full_cycle() {
    // Set up: refinery + maintenance bay + repair kits + ore
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let mut rng = make_rng();

    // Run enough ticks for refinery to fire several times and accumulate wear
    for _ in 0..20 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let station = &state.stations[&station_id];
    let refinery = &station.modules[0];

    // Refinery should have some wear (accumulated) but not be at 1.0
    // because maintenance bay is repairing it
    assert!(refinery.wear.wear > 0.0, "refinery should have accumulated some wear");
    assert!(refinery.enabled, "refinery should still be enabled (maintenance keeping up)");

    // Some repair kits should have been consumed
    let kits = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Component { component_id, count, .. } = i {
            if component_id.0 == "repair_kit" { Some(*count) } else { None }
        } else { None }
    }).unwrap_or(0);
    assert!(kits < 5, "some repair kits should have been consumed");
}
```

**Step 2: Run tests**

Run: `cargo test -p sim_core test_wear_maintenance_full_cycle`
Expected: PASS.

**Step 3: Commit**

```bash
git add -A && git commit -m "test(wear): integration test — wear accumulation + maintenance repair cycle"
```
