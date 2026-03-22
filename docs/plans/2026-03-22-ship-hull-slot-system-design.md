# Ship Hull+Slot System Design

**Goal:** Content-driven ship hull classes with typed module slots, equipment fitting, and stat computation via the modifier pipeline — the foundational entity architecture for ship diversity and progression.
**Status:** Planned
**Linear Project:** [Ship Hull+Slot System](https://linear.app/violetspacecadet/project/ship-hullslot-system-e5442c0e7117)

## Overview

The simulation currently has one ship type (mining shuttle) with no customization. All ships are identical stat platforms — same speed, same cargo, same mining capability. There is no strategic decision space around fleet composition. Players/autopilot build "more of the same" rather than choosing *what* to build.

This project introduces hull classes that define ship archetypes (Mining Barge, Transport Hauler, Survey Scout, General Purpose) with typed module slots. Ship modules are manufactured items fitted into slots to customize capabilities. Hull bonuses and module modifiers flow through the existing StatModifier system (VIO-332) to compute effective ship stats. The architecture is content-driven — adding a new hull class or ship module is a JSON change, not a code change.

This is the first of three entity depth sub-projects. The ship hull+slot system establishes the shared slot architecture pattern that station frames will reuse. The template/blueprint system (third sub-project) depends on both being designed.

## Design

### Data Model

#### New newtypes

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HullId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SlotType(pub String);  // Content-driven, not a Rust enum

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModuleDefId(pub String);  // Replaces bare String references to module defs
```

SlotType is a content-driven string newtype (following AnomalyTag/DataKind pattern). Adding a new slot type = JSON content only. ModuleDefId provides type safety for the two reference sites: station InstallModule commands and ship FittedModule entries.

#### HullDef (new, content-driven)

```rust
pub struct HullDef {
    pub id: HullId,
    pub name: String,
    pub mass_kg: f32,
    pub cargo_capacity_m3: f32,
    pub base_speed_ticks_per_au: u64,
    pub base_propellant_capacity_kg: f32,  // Built-in tank (short-range hops only)
    pub base_exhaust_velocity_m_s: f32,    // Engine efficiency (Isp × g₀)
    pub slots: Vec<SlotDef>,
    pub bonuses: Vec<Modifier>,           // Static modifiers → ship's ModifierSet
    pub required_tech: Option<TechId>,
    pub tags: Vec<String>,                // Presentation-only
}

pub struct SlotDef {
    pub slot_type: SlotType,
    pub label: String,                    // "High Slot 1", "Utility Bay A"
}
```

Slot index is implicit (position in the `slots` vec). SlotDef is intentionally minimal — no power budget per slot, no size constraints. Those can be added later without breaking the schema.

Note on slot_index fragility: slot indices are structural positions on a hull, so reordering is an authoring error rather than normal content evolution. If `slot_id: String` is added to SlotDef later, the migration path mirrors the recipe selection migration (index → ID).

#### ModuleDef additions

```rust
pub struct ModuleDef {
    // ... existing fields (name, behavior, wear_per_run, power, thermal) ...
    pub compatible_slots: Vec<SlotType>,  // NEW: which slot types accept this module
    pub ship_modifiers: Vec<Modifier>,    // NEW: modifiers applied when fitted to a ship
}
```

`compatible_slots` defaults to empty (`#[serde(default)]`) — existing station modules have no slot compatibility, can't be fitted to ships. `ship_modifiers` are stat bonuses a module provides when fitted (e.g., mining laser → +MiningRate PctAdditive 0.25).

#### New ModuleBehaviorDef variant

```rust
pub enum ModuleBehaviorDef {
    // ... existing variants ...
    Equipment,  // No tick behavior — passive stat provider for ships
}
```

Equipment modules don't tick. They exist to carry `ship_modifiers` and `compatible_slots`. Examples: mining laser, cargo expander, propulsion engine, scanner.

Equipment modules are exempt from wear in Phase 1 (no tick = no `wear_per_run` path). Future: passive wear mechanism (time-based or usage-based, e.g., mining laser wears per mine task completion) needed to preserve entropy theme.

#### ShipState additions

```rust
pub struct ShipState {
    // ... existing fields ...
    pub hull_id: HullId,                        // NEW: hull class reference
    pub fitted_modules: Vec<FittedModule>,       // NEW: modules in hull slots
    pub propellant_kg: f32,                      // NEW: current fuel level (mutates on transit)
    // cargo_capacity_m3, speed_ticks_per_au, propellant_capacity_kg, exhaust_velocity_m_s
    // remain as stored fields, recomputed from hull + modules when fitting changes
}

pub struct FittedModule {
    pub slot_index: usize,
    pub module_def_id: ModuleDefId,
}
```

Ship stats are cached, not computed per-tick. When hull is assigned or modules are fitted/unfitted, `recompute_ship_stats()` recomputes `cargo_capacity_m3`, `speed_ticks_per_au`, `propellant_capacity_kg`, and `exhaust_velocity_m_s` from hull base + all fitted module modifiers + hull bonuses. This preserves the existing interface — all code that reads these fields works unchanged.

`propellant_kg` is the one mutable propellant field — it changes on every transit (deducted) and refuel (replenished). `propellant_capacity_kg` is the ceiling, computed from hull's `base_propellant_capacity_kg` + tank module modifiers.

`dry_mass_kg` is NOT stored — it's computed on demand as `hull.mass_kg + sum(fitted_module_masses)`. This avoids a stale cached value for a field that changes only on fitting and is only needed for Tsiolkovsky calculations.

#### New StatId variant

```rust
pub enum StatId {
    // ... existing ...
    CargoCapacity,       // NEW: for cargo expander modules
    PropellantCapacity,  // NEW: for propellant tank modules
    ExhaustVelocity,     // NEW: for propulsion modules (engine efficiency)
}
```

Three new StatIds for hull+slot and propellant integration. Most other ship module stats already have StatIds (MiningRate, ShipSpeed, ScanDuration).

**Inverse stat convention:** ShipSpeed (ticks_per_au) and ScanDuration are both "duration" stats — lower is better. Negative PctAdditive = faster. Documented in content file headers. If confusing in practice, stats can be wrapped later without breaking modifiers.

#### ModifierSource additions

```rust
pub enum ModifierSource {
    // ... existing variants ...
    Hull(HullId),                              // NEW: hull bonuses
    FittedModule(ModuleDefId, usize),          // NEW: fitted module modifiers (def + slot index)
}
```

Separate variants (not overloaded Equipment) enable targeted removal: clear hull bonuses independently of module modifiers, remove modifiers for a specific slot on module swap without rebuilding the entire set.

#### GameContent addition

```rust
pub struct GameContent {
    // ... existing fields ...
    pub hulls: BTreeMap<HullId, HullDef>,  // NEW: hull catalog
}
```

BTreeMap for deterministic iteration, consistent with the recipe catalog pattern.

#### Stat recomputation

```rust
fn recompute_ship_stats(ship: &mut ShipState, hull: &HullDef, content: &GameContent) {
    // Clear all hull and fitted module modifiers
    ship.modifiers.remove_by_source_prefix(|s| matches!(s, ModifierSource::Hull(_) | ModifierSource::FittedModule(_, _)));

    // Apply hull bonuses
    for bonus in &hull.bonuses {
        ship.modifiers.add(Modifier {
            source: ModifierSource::Hull(hull.id.clone()),
            ..bonus.clone()
        });
    }

    // Apply fitted module modifiers
    for fitted in &ship.fitted_modules {
        if let Some(module_def) = content.module_defs.get(&fitted.module_def_id) {
            for modifier in &module_def.ship_modifiers {
                ship.modifiers.add(Modifier {
                    source: ModifierSource::FittedModule(fitted.module_def_id.clone(), fitted.slot_index),
                    ..modifier.clone()
                });
            }
        }
    }

    // Recompute cached stats
    ship.cargo_capacity_m3 = ship.modifiers.resolve_f32(StatId::CargoCapacity, hull.cargo_capacity_m3);
    ship.speed_ticks_per_au = Some(
        ship.modifiers.resolve(StatId::ShipSpeed, hull.base_speed_ticks_per_au as f64) as u64
    );
    ship.propellant_capacity_kg = ship.modifiers.resolve_f32(
        StatId::PropellantCapacity, hull.base_propellant_capacity_kg
    );
    ship.exhaust_velocity_m_s = ship.modifiers.resolve_f32(
        StatId::ExhaustVelocity, hull.base_exhaust_velocity_m_s
    );
    // Clamp current propellant to new capacity (tank module removed mid-flight)
    ship.propellant_kg = ship.propellant_kg.min(ship.propellant_capacity_kg);
}
```

Called from every path that mutates `fitted_modules` — FitShipModule, UnfitShipModule, and hull assignment. Debug assertion in tick loop: `debug_assert!(cached stats match fresh recomputation)`. Zero runtime cost in release builds. Catches missed recomputation paths.

### Tick Ordering

No new tick phase. Ship modules don't tick — they're passive stat providers.

Stat recomputation happens during `apply_commands()` (tick step 1), triggered by fitting commands. Every path that mutates `fitted_modules` calls `recompute_ship_stats()`.

### Commands

```rust
Command::FitShipModule {
    ship_id: ShipId,
    slot_index: usize,
    module_def_id: ModuleDefId,
    station_id: StationId,         // Station supplying the module
}

Command::UnfitShipModule {
    ship_id: ShipId,
    slot_index: usize,
    station_id: StationId,         // Station receiving the module back
}
```

**Validation (at command application time):**
- Ship must be at the station's location (position check)
- Ship must be Idle (can't refit mid-task)
- Slot index must be valid for the hull
- Module def must exist and have `compatible_slots` containing the hull slot's `slot_type`
- Module must exist in station inventory as `InventoryItem::Module { module_def_id }`
- Slot must be empty (FitShipModule) or occupied (UnfitShipModule)

FitShipModule on an occupied slot is rejected — require explicit UnfitShipModule first. No implicit swap. Simpler, more explicit, fully auditable.

**Command ordering guarantees:** Player commands are queued before autopilot runs, so FitShipModule processes before autopilot's AssignShipTask — there is always a window for player-issued fitting commands. For autopilot-initiated fitting, UnfitShipModule + FitShipModule are adjacent in the same command batch, processed sequentially with no intervening code.

### Events

```rust
Event::ShipModuleFitted {
    ship_id: ShipId,
    slot_index: usize,
    module_def_id: ModuleDefId,
    station_id: StationId,
}

Event::ShipModuleUnfitted {
    ship_id: ShipId,
    slot_index: usize,
    module_def_id: ModuleDefId,
    station_id: StationId,
}
```

### Ship Construction Change

OutputSpec::Ship updated to reference hull exclusively:

```rust
OutputSpec::Ship { hull_id: HullId }
```

`cargo_capacity_m3` removed — hull's `cargo_capacity_m3` is the sole source of truth. Existing shipyard recipe content migrated in a one-time edit.

New ships created with `hull_id` set, empty `fitted_modules`, and stats computed from hull def. Autopilot (or future player) fits modules after construction.

### SSE / API / Frontend Integration

**`GET /api/v1/content` additions:** Hull catalog served with all fields (id, name, slots, bonuses, tags). Module defs now include `compatible_slots` and `ship_modifiers`.

**New Events:** `ShipModuleFitted`, `ShipModuleUnfitted`.

**`GET /api/v1/state`:** ShipState now includes `hull_id` and `fitted_modules`.

**No new endpoints.** All changes fit within existing content/state/command surface.

**Frontend:**
- `applyEvents.ts` — handlers for `ShipModuleFitted` and `ShipModuleUnfitted`
- `eventSchemas.ts` — Zod schemas for new event variants
- `ci_event_sync.sh` — add new Event variants
- `types.ts` — ShipState adds `hull_id`, `fitted_modules` fields
- `FleetPanel.tsx` / `ShipDetail.tsx` — ship detail view shows hull class name, fitted modules list with slot labels. Phase 1: read-only display. Fitting UI deferred to a future mockup pass.

### Autopilot Fitting

**New behavior: `ShipFittingBehavior`** — runs every tick, checking all idle ships at stations for empty slots with available modules in inventory.

**Fitting templates** (`content/fitting_templates.json`): maps hull_id → preferred loadout (ordered list of `{ slot_index, module_def_id }` pairs). Hull definitions stay purely structural — autopilot strategies are separate from hull properties.

Example:
```json
{
  "hull_mining_barge": [
    { "slot_index": 0, "module_def_id": "module_mining_laser" },
    { "slot_index": 1, "module_def_id": "module_mining_laser" },
    { "slot_index": 2, "module_def_id": "module_survey_scanner" }
  ]
}
```

**Retrofit handling:** Because the behavior runs every tick (not just at construction), ships built before modules are manufactured get fitted on the first tick after modules appear in station inventory and the ship is idle.

**Multi-ship determinism:** Ships processed in ID order (ascending). First in order gets first pick from inventory. Falls out naturally from sorted iteration.

**Fitting template validation at load time:** template references a module that doesn't exist → panic. Template slot index doesn't exist on hull → panic. Same pattern as recipe validation catching dangling IDs.

### Content Files

**New file: `content/hull_defs.json`** — flat array of HullDef objects.

| Hull | Base Cargo (m3) | Base Speed (ticks/AU) | Base Propellant (kg) | Base Ve (m/s) | Slots | Bonuses |
|---|---|---|---|---|---|---|
| `hull_general_purpose` | 50.0 | 120 | 10,000 | 30,000 | 2 utility, 1 industrial | None (balanced baseline) |
| `hull_mining_barge` | 80.0 | 180 | 8,000 | 25,000 | 2 industrial, 1 utility | +25% MiningRate (PctAdditive) |
| `hull_transport_hauler` | 200.0 | 150 | 15,000 | 30,000 | 1 utility, 2 propulsion | +50% CargoCapacity (PctAdditive) |
| `hull_survey_scout` | 20.0 | 80 | 5,000 | 35,000 | 3 utility | -30% ScanDuration (PctAdditive, -0.3) |

Base propellant is intentionally small — enough for short-range hops within a local zone. Cross-system travel requires propellant tank modules. Survey scout has the smallest tank but highest exhaust velocity (fuel-efficient). Transport hauler has the largest base tank (needs range for cargo runs). Mining barge has low tank and low Ve — designed to operate near the station, not cross the system.

**New ship modules (added to `content/module_defs.json`):**

| Module | Behavior | Slot Type | Ship Modifiers | Recipe |
|---|---|---|---|---|
| `module_mining_laser` | Equipment | industrial | MiningRate +20% PctAdditive | 2× fe_plate + 1× thruster |
| `module_cargo_expander` | Equipment | utility | CargoCapacity +30% PctAdditive | 3× structural_beam |
| `module_survey_scanner` | Equipment | utility | ScanDuration -15% PctAdditive | 2× fe_plate |
| `module_basic_engine` | Equipment | propulsion | ShipSpeed -15% PctAdditive (faster) | 2× thruster + 1× fe_plate |
| `module_propellant_tank` | Equipment | utility | PropellantCapacity +50% PctAdditive | 2× structural_beam + 1× fe_plate |

Ship module recipes reference intermediates from the Manufacturing DAG project (fe_plate, structural_beam) and existing components (thruster). Circuit deferred to a content expansion.

The propellant tank creates a core fitting tradeoff: a mining barge with 2 industrial + 1 utility slot can fit 2 mining lasers + 1 tank (range) or 2 mining lasers + 1 scanner (awareness) or 1 mining laser + 1 cargo expander + 1 tank. Range is a scarce resource allocated through slot decisions.

**New file: `content/fitting_templates.json`** — autopilot fitting strategies per hull.

**Updated: `content/module_defs.json`** — existing modules get `compatible_slots: []` (station-only). New ship modules get slot compatibility.

**Updated: `content/component_defs.json`** — ship modules as assembled components need ComponentDef entries with mass/volume.

**Updated: `content/dev_base_state.json`** — initial ship gets `hull_id: "hull_general_purpose"`, `fitted_modules: []`.

**Updated: shipyard recipe** — `OutputSpec::Ship { hull_id: "hull_general_purpose" }`.

**Content validation at load time:**
1. Hull ID uniqueness — panic on duplicates
2. Hull slot types that have no compatible modules — **warning** (not panic). Forward-declaring slot types for future content is valid.
3. Hull bonuses reference valid StatIds — panic on invalid
4. Module `compatible_slots` values not present in any hull — **warning** (not panic). Same rationale.
5. `OutputSpec::Ship { hull_id }` references a valid hull — panic on missing
6. Fitting template validation — module def not found or slot index invalid → panic

### Migration / Backwards Compatibility

- `ShipState.hull_id` — `#[serde(default)]` defaulting to `HullId("hull_general_purpose".into())`. Existing ships become General Purpose.
- `ShipState.fitted_modules` — `#[serde(default)]` → empty vec. Existing ships start unfitted.
- `ShipState.propellant_kg` — `#[serde(default)]` → 0.0. Existing ships start empty (recompute fills from hull base on first fitting pass). Alternative: default to hull's `base_propellant_capacity_kg` (starts full).
- `ShipState.propellant_capacity_kg` — cached, recomputed from hull. `#[serde(default)]`.
- `ShipState.exhaust_velocity_m_s` — cached, recomputed from hull. `#[serde(default)]`.
- `ModuleDef.compatible_slots` — `#[serde(default)]` → empty vec. Existing station modules unchanged.
- `ModuleDef.ship_modifiers` — `#[serde(default)]` → empty vec.
- `OutputSpec::Ship` — content migration: existing shipyard recipe changes to `{ "ship": { "hull_id": "hull_general_purpose" } }`. One-time content file edit.
- `ModifierSource` — two new variants (`Hull`, `FittedModule`). Existing serialized ModifierSets unaffected.
- `StatId::CargoCapacity`, `PropellantCapacity`, `ExhaustVelocity` — new variants. Existing deserialization unaffected.
- `ModuleDefId` newtype — internal refactor, serializes as same string. No save format change.

**No breaking changes to saves.** Old saves load with general purpose hull, no fitted modules. Existing ship behavior preserved.

## Testing Plan

- **Unit tests** (sim_core): hull catalog loading + validation (missing ID panics, slot type warnings, bonus StatId validation), FitShipModule/UnfitShipModule command processing (valid/invalid: wrong slot type, occupied slot, ship not at station, ship not idle, module not in inventory), stat recomputation (fit mining laser → verify MiningRate, fit cargo expander → verify CargoCapacity), debug assertion (cached stats match fresh recomputation), ModuleDefId newtype roundtrip serialization, new ModifierSource variants serialization + remove_by_source targeting, OutputSpec::Ship with hull_id → new ship gets correct hull, CargoCapacity StatId resolution
- **Hull bonus modifier lifecycle test**: assign mining barge hull → verify MiningRate bonus → fit module → verify both hull bonus and module modifier present → unfit module → verify hull bonus still active. Catches recompute_ship_stats accidentally clearing hull bonuses during module changes.
- **Fitting template validation negative test**: template referencing non-existent module or invalid slot index → panic at load time. Same pattern as recipe validation.
- **Integration tests**: ship construction → autopilot fitting (fills slots from inventory, stats reflect modules), multi-ship construction determinism (two ships same tick, limited modules → allocation by ship ID), full fitting lifecycle (construct → fit → verify → unfit → verify revert → refit), retrofit timing (fits on first tick after module appears in inventory and ship is idle at station)
- **Determinism regression**: sim_bench scenario with hull system. Two seeds, identical final state (hull assignments, fitted modules, computed stats).
- **sim_bench scenario**: `scenarios/hull_fitting.json` — concrete assertions: at least 2 hull types constructed by tick 3000, all ships have ≥1 fitted module by tick 5000, all mining barges have resolved MiningRate > base MiningRate
- **Frontend**: vitest for Zod schemas (ShipModuleFitted, ShipModuleUnfitted), vitest for hull data parsing from content API. Chrome-based visual testing deferred to hull display implementation.

## Ticket Breakdown

### Ship Hull+Slot System

1. **SH-01: Data model + content loading** — HullId/SlotType/ModuleDefId newtypes, HullDef struct (includes base_propellant_capacity_kg, base_exhaust_velocity_m_s), hull_defs.json loading + BTreeMap on GameContent, ModuleDef additions (compatible_slots, ship_modifiers), Equipment behavior variant, ModifierSource::Hull + FittedModule variants, StatId::{CargoCapacity, PropellantCapacity, ExhaustVelocity}, content validation, content endpoint hull catalog
2. **SH-02: Fitting commands + stat computation** — ShipState hull_id/fitted_modules/propellant_kg fields, FitShipModule/UnfitShipModule commands + validation, recompute_ship_stats() (cargo, speed, propellant capacity, exhaust velocity), debug assertion, OutputSpec::Ship hull_id migration, ShipModuleFitted/ShipModuleUnfitted events
3. **SH-03: Hull + ship module content** — 4 hull classes in hull_defs.json (with propellant/Ve stats), 5 ship modules in module_defs.json (including propellant tank), ComponentDef entries, fitting_templates.json, dev_base_state.json update, shipyard recipe migration, pricing updates
4. **SH-04: Autopilot fitting behavior** — ShipFittingBehavior (every-tick idle ship check), fitting template loading + validation (negative tests), retrofit trigger, multi-ship deterministic allocation
5. **SH-05: SSE + frontend data layer** — Zod schemas for ShipModuleFitted/ShipModuleUnfitted, applyEvents handlers, types.ts hull/fitting fields, FleetPanel hull display (read-only), ci_event_sync.sh update
6. **SH-06: Testing + determinism validation** — Unit tests, hull bonus modifier lifecycle, fitting template validation negatives, integration tests (multi-ship, retrofit timing, full lifecycle), determinism regression, sim_bench hull_fitting.json scenario

Dependencies: SH-01 → SH-02, SH-03; SH-02 + SH-03 → SH-04; SH-02 + SH-03 → SH-05; all → SH-06

## Open Questions

- **Equipment wear**: Phase 1 equipment modules don't wear. Future passive wear mechanism needed (time-based or usage-based). Not Phase 1.
- **slot_id migration**: slot_index: usize is acceptable for structural positions. If slot_id: String is needed later, migration path mirrors recipe selection.
- **Circuit component**: Ship module recipes simplified to use existing intermediates. Circuit as a new intermediate deferred to content expansion (either Manufacturing DAG content ticket or separate).
- **Inverse stat convention**: ShipSpeed and ScanDuration are "duration" stats (lower = better). Documented in content file headers. Can be wrapped with positive-means-better stats later if confusing.
- **Fitting UI**: Phase 1 is read-only hull/module display. Interactive fitting UI deferred to a visual mockup pass.
- **Autopilot hull selection**: Phase 1 autopilot builds from default shipyard recipe (General Purpose). Selecting which hull to build based on fleet needs (need more mining → build Mining Barge) deferred to station frames/templates sub-project or Phase 2.

## Epic 4 (Propellant-Based Movement) Reconciliation

This project supersedes VIO-117's data model. Epic 4 was designed assuming flat ship fields (`dry_mass_kg`, `propellant_capacity_kg`, `exhaust_velocity_m_s` as constants on ShipState). With hull+slot:

- **`dry_mass_kg`** — no longer stored. Computed on demand: `hull.mass_kg + sum(fitted_module_masses)`. Changes with every fitting change. Needed only for Tsiolkovsky calculations.
- **`propellant_capacity_kg`** — cached on ShipState, recomputed from `hull.base_propellant_capacity_kg` + PropellantCapacity modifiers (from tank modules). Base tanks are intentionally small (short-range hops). Cross-system travel requires tank modules.
- **`exhaust_velocity_m_s`** — cached on ShipState, recomputed from `hull.base_exhaust_velocity_m_s` + ExhaustVelocity modifiers (from engine modules).
- **`propellant_kg`** — still a mutable field on ShipState (deducted on transit, replenished on refuel).
- **`total_mass_kg()`** — still `dry_mass + propellant + cargo`. But dry_mass is now computed from hull + modules.

**Epic 4 tickets that need revision:**
- **VIO-117** — significant rewrite: flat fields → hull-derived computed values. Should depend on hull+slot landing first.
- **VIO-118** — minor: `total_mass_kg()` inputs change sources. LUT and Tsiolkovsky math unchanged.
- **VIO-123** — absorbed by SH-03 (dev state with hull-aware values).
- **VIO-119, VIO-120, VIO-121, VIO-122** — mostly unchanged, use computed values instead of flat fields.

**Ordering:** Ship Hull+Slot System should land before Epic 4. Hull+slot establishes the entity model; propellant plugs into it.
