# Heat & High-Temperature Materials — Design & Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a heat system where temperature is a managed resource — modules have thermal state, recipes can require temperature ranges, molten materials flow through discrete batch transfers, and thermal mismanagement creates compounding wear pressure.

**Architecture:** Lumped-parameter thermal model. Per-module ThermalState (temp in milli-Kelvin u32). Per-batch MaterialThermalProps only inside thermal-aware containers. Discrete batch transfers along explicit port/link connections. Linearized cooling. All integer arithmetic for determinism.

**Tech Stack:** Rust (sim_core types + station tick), JSON content definitions, React UI components.

---

## Updated Decision Log

| # | Decision | Choice | Status | Rationale |
|---|----------|--------|--------|-----------|
| 1 | Fun vs realism | B — Managed resource, not physics sim | **Confirmed** | Design Spine 2.1: no real thermodynamics |
| 2 | Energy dependency | A — Finish power enforcement first | **Confirmed** | Heat modules consume power; budget must exist |
| 3 | Temperature locality | D — Hybrid: per-module + per-batch in thermal containers | **Confirmed** | Modules own thermal state; materials carry temp only when in crucibles/molds |
| 4 | Molten flow model | A + thin port/link — Discrete batches along explicit links | **Confirmed** | No flow solver. Links are station-level config. Deterministic ordering. |
| 5 | Overheating severity | C — Soft degradation + stalling + recoverable damage events | **Confirmed** | Threshold-based, observable, economically costly but not destructive |
| 6 | First use-case | A (smelter MVP) → C (casting v1) → D (propellant deferred) | **Confirmed** | Sequential unlock: thermal foundation, then molten materials |
| 7 | UI priority | Numeric readout + state badge + alerts first; overlay later | **Confirmed** | |
| 8 | Temperature unit | milli-Kelvin u32, all arithmetic in u64 intermediates | **Decided** | Zero float drift, sufficient range (0–4.29M K) |
| 9 | Thermal model | Lumped parameter per-module | **Decided** | Design spine says no heavy physics |
| 10 | Radiative cooling | Linearized: Q_loss = k * (T - T_sink) | **Decided** | Gameplay-equivalent, cheap, tunable |
| 11 | Save/load compat | Optional fields with serde default | **Decided** | Standard pattern in codebase |
| 12 | Thermal zones | ThermalGroupId on modules, default = station-wide | **Decided** | Avoids future refactors; radiators/exchangers target group |
| 13 | Sink temperature | T_sink_mk default = 293_000 (20°C), not cosmic 3K | **Decided** | Realistic space background deferred to exposed-module variant |
| 14 | Energy↔thermal units | Power in Watts, dt_s from minutes_per_tick, Q_j = P_w * dt_s | **Decided** | Clean contract, no mixed units |
| 15 | Latent heat | On MaterialThermalProps, not module | **Decided** | Phase change is property of material batch |
| 16 | Port/link abstraction | Modules declare ports; station stores Links; transfers along Links | **Decided** | Thin layer, no routing, no solver |

---

## Finalized Architecture Summary

### 0. Determinism Invariants

**No floats in sim-state math.** f32/f64 are only permitted in content definitions (e.g., `solar_intensity`, `base_output_kw`) and display-only fields. All sim-state arithmetic (temperature, heat flow, phase transitions) uses integer types (u32 milli-Kelvin, i64 Joules, u64 intermediates). Content floats are converted to integer representations at load time or at the boundary of the thermal tick step.

**Travel-time-locked-at-departure determinism.** Thermal tasks follow the same philosophy as transit and mining: all parameters are computed and locked at task start. A TransferMolten command computes material temperature at dispatch time; mid-transfer temperature changes do not affect the batch in flight. This matches the existing pattern where propellant is deducted atomically at transit start.

### 1. Canonical Time Contract

```
minutes_per_tick = constants.minutes_per_tick   // currently 1.0, VIO-187 proposes 60
dt_s = minutes_per_tick * 60.0                  // seconds per tick
```

All power is in Watts (J/s). All heat quantities in Joules.
Per-tick energy: `Q_j = P_w * dt_s`

This contract is established ONCE in a helper function and used everywhere.
If VIO-187 lands first, dt_s changes automatically. If not, dt_s = 60.0 (1 min × 60 s/min).

### 2. ThermalState (on ModuleState)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThermalState {
    pub temp_mk: u32,                           // milli-Kelvin, default 293_000
    pub thermal_group: Option<ThermalGroupId>,   // None = station default group
}
```

Only present on modules with `ThermalDef` in their module definition. Non-thermal modules have `thermal: None` on `ModuleState`.

### 3. ThermalDef (on ModuleDef, in content)

```rust
pub struct ThermalDef {
    pub heat_capacity_j_per_k: f32,         // How much energy to change temp by 1K
    pub passive_cooling_coefficient: f32,   // W/K — heat loss rate to sink
    pub max_temp_mk: u32,                   // Damage threshold
    pub operating_min_mk: Option<u32>,      // Required for thermal recipes
    pub operating_max_mk: Option<u32>,      // Quality degrades above this
    pub thermal_group: Option<ThermalGroupId>, // Default group assignment
}
```

### 4. ThermalGroupId

```rust
pub type ThermalGroupId = String;  // e.g., "default", "smelting_bay", "cryo_section"
```

- Every module belongs to a thermal group (default: `"default"`)
- Radiators cool all modules in their group
- Heat exchangers transfer between groups
- Group assignment is on the module def (content) with runtime override via command

### 5. MaterialThermalProps (on InventoryItem)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialThermalProps {
    pub temp_mk: u32,
    pub phase: Phase,
    pub latent_heat_buffer_j: i64,  // Tracks partial phase transitions
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Solid,
    Liquid,
}
```

Only present on materials inside thermal-aware containers (crucibles, casting molds).
Regular inventory items have `thermal: None` — assumed ambient solid.

### 6. Element Thermal Content

```json
// In elements.json, new optional fields:
{
  "id": "Fe",
  "melting_point_mk": 1811000,
  "latent_heat_j_per_kg": 247000,
  "specific_heat_j_per_kg_k": 449
}
```

### 7. Port/Link Abstraction (thin — no routing, no solver, no pathfinding)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePort {
    pub id: String,           // e.g., "molten_out", "molten_in"
    pub direction: PortDirection,
    pub accepts: PortFilter,  // What can flow through
}

pub enum PortDirection { Input, Output }

pub enum PortFilter {
    AnyMolten,
    Element(ElementId),
    Phase(Phase),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalLink {
    pub id: String,
    pub from_module: ModuleId,
    pub from_port: String,
    pub to_module: ModuleId,
    pub to_port: String,
}
```

- Links stored on `StationState` as `Vec<ThermalLink>`
- Transfers are commands: `TransferMolten { link_id, kg }` — moves a discrete batch
- **Deterministic iteration order:** links processed sorted by `(from_module_id, to_module_id, port index)` — never by insertion order
- **Explicitly NOT:** No routing logic. No flow solver. No pathfinding. No automatic flow. No continuous rates. Links are static declarations, transfers are explicit commands.

### 8. Thermal Tick Step

Inserted as step 3.6 in tick loop (after maintenance, before research):

```
For each station:
  // Group modules by ThermalGroupId
  For each thermal group (sorted by group ID):
    For each module in group (sorted by module ID):
      1. Heat generation: if module ran this tick, add Q_gen = heat_per_run_j
      2. Passive cooling: Q_loss = passive_cooling_coefficient * dt_s * (T - T_sink) / 1000
         (divide by 1000 because T is in milli-K)
      3. Radiator cooling: sum radiator capacity in this group
         Q_rad = min(radiator_total_w * dt_s, needed_to_reach_target)
      4. Net heat: Q_net = Q_gen - Q_loss - Q_rad
      5. Temp update: delta_mk = (Q_net * 1000) / heat_capacity_j_per_k
         temp_mk = clamp(temp_mk + delta_mk, T_SINK_MK, T_MAX_ABSOLUTE)
      6. Overheat check: apply wear multiplier, stall if critical

  // Thermal container cooling (crucibles, molds)
  For each thermal inventory item (sorted by container module ID, then item index):
    Apply cooling based on container's insulation
    Check phase transitions (latent heat buffer)
```

### 9. Overheat Escalation Bands

All thresholds relative to module's `max_temp_mk`:

| Zone | Condition | Effect |
|------|-----------|--------|
| Below operating_min | temp < operating_min_mk | Module stalls (too cold for thermal recipes) |
| Operating range | min ≤ temp ≤ max | Normal operation. Yield interpolated by temp position. |
| Warning | max < temp ≤ max + 200_000 | Wear rate ×2. Quality drops. `OverheatWarning` event. |
| Critical | max + 200_000 < temp ≤ max + 500_000 | Wear rate ×4. Module auto-stalls. `OverheatCritical` event. |
| Damage | temp > max + 500_000 | Wear jumps to critical band (0.8). Molten spill risk (material lost). `OverheatDamage` event. |

All transitions emit events. All are observable before triggering (warning zone exists).
Damage is economically costly (repair kits + lost material) but never permanently destroys a module.

### 10. Recipe Thermal Requirements

```rust
// Optional field on RecipeDef
pub struct RecipeThermalReq {
    pub min_temp_mk: u32,
    pub optimal_min_mk: Option<u32>,  // Optimal range start
    pub optimal_max_mk: Option<u32>,  // Optimal range end
    pub max_temp_mk: Option<u32>,     // Above this, quality degrades
    pub heat_per_run_j: i64,          // Positive = exothermic, negative = endothermic
}
```

Yield formula when thermal requirements exist:
- Below min: recipe does not run (stall)
- min to optimal_min: yield scales linearly from 80% to 100%
- optimal range: 100% yield
- optimal_max to max: quality degrades linearly from 100% to 60%
- Above max: quality severely degraded (30%), triggers warning

### 11. New Module Types

| Module | Type | Thermal Behavior | Power Draw |
|--------|------|------------------|------------|
| **Smelter** | Processor + ThermalDef | Generates heat on recipe run. Requires min operating temp. | High (30 kW) |
| **Radiator Panel** | New behavior: Radiator | Removes heat from thermal group. Wear reduces capacity. | 0 |
| **Crucible** | New behavior: ThermalContainer | Holds molten material. Insulated (slow heat loss). Has ports. | 0 |
| **Casting Mold** | Processor + ThermalDef | Receives molten input, cools → solid. Exothermic (releases heat). | Low (5 kW) |
| **Heat Exchanger** | New behavior: HeatExchanger | Transfers heat between thermal groups. Active device. | Moderate (15 kW) |

### 12. Metrics

| Metric | Type | Phase |
|--------|------|-------|
| `station_max_temp_mk` | gauge | MVP |
| `station_avg_temp_mk` | gauge | MVP |
| `overheat_warning_count` | counter | MVP |
| `overheat_critical_count` | counter | MVP |
| `overheat_damage_count` | counter | v1 |
| `molten_spill_kg` | counter | v1 |
| `pipe_freeze_count` | counter | v1 |
| `radiator_utilization_pct` | gauge | MVP |
| `heat_wear_multiplier_avg` | gauge | MVP |
| `smelter_yield_efficiency` | gauge | MVP |

### 13. Save/Load Schema

All new fields use `Option<T>` with `#[serde(default)]`:

```rust
// ModuleState gains:
pub thermal: Option<ThermalState>,

// InventoryItem::Material gains:
pub thermal: Option<MaterialThermalProps>,

// StationState gains:
pub thermal_links: Vec<ThermalLink>,  // #[serde(default)]
```

Backward compatible: missing fields default to None/empty. Existing saves load without migration.

---

## Sequencing Plan

### Prerequisites: Energy Enforcement (must complete first)

These existing tickets from "Epic 1: Energy System Foundation" must land before heat work begins:

| Ticket | Title | Status | Why Required |
|--------|-------|--------|--------------|
| VIO-88 | E1-05: Power budget computation in tick loop | **In Progress** | Heat modules consume power; budget must be computed |
| VIO-89 | E1-06: Enforce power consumption — stall modules on deficit | Backlog | Smelter/exchanger must stall on power deficit |
| VIO-90 | E1-07: Battery module behavior type | Backlog | Batteries buffer power for intermittent smelter operation |
| VIO-91 | E1-08: Add battery + solar to dev_base_state | Backlog | Starting state needs power infrastructure |
| VIO-92 | E1-09: Power metrics to MetricsSnapshot | Backlog | Thermal metrics build on power metrics infrastructure |
| VIO-95 | E1-12: FE — power bar in station panel | Backlog | Thermal UI builds on power UI components |

**Hard gate:** H1-07 (add smelter to dev_base_state) and any runtime enforcement of thermal power draw **must wait** for VIO-88 + VIO-89 to land. H0 (types + constants) can start immediately and land safely before power enforcement is complete. Battery (VIO-90) and FE power UI (VIO-95) can land in parallel with early heat tickets.

### Phase 0: Thermal Foundation Types (no tick behavior yet)

Add types, content schema, and test infrastructure. Nothing runs yet.

### Phase MVP: Smelter + Radiator + Temp Gating

First playable thermal content. Smelter requires temperature to run. Radiator cools. Overheating damages.

### Phase v1: Molten Materials + Port/Link + Casting

Materials carry temperature and phase. Crucibles hold molten metal. Casting molds produce components. Port/link abstraction enables directed transfers.

### Phase v2: Heat Exchanger + Steel + Advanced Thermal

Heat exchangers transfer between zones. Steel alloy recipe. Insulated containers. Autopilot thermal awareness.

---

## Linear Project Structure

### Project: Heat & Molten Materials

**Epics:**

1. **H0: Thermal Foundation Types** — Types, content schema, constants. No tick behavior.
2. **H1: Thermal Tick + Smelter MVP** — Thermal tick step, smelter module, radiator, temp gating.
3. **H2: Overheat Escalation** — Warning/critical/damage bands, wear multiplier, events.
4. **H3: Thermal Metrics + Alerts** — MetricsSnapshot fields, daemon alerts, sim_bench output.
5. **H4: FE Thermal UI** — Temperature readouts, state badges, alert integration.
6. **H5: Port/Link Abstraction** — Module ports, station links, transfer command.
7. **H6: Molten Materials** — MaterialThermalProps, phase transitions, crucible, casting mold.
8. **H7: Testing & Benchmarks** — Integration scenarios, determinism, regression.
9. **H8: Documentation** — Design doc updates, reference.md, CLAUDE.md.

### Ticket Breakdown

---

#### Epic H0: Thermal Foundation Types

**H0-01: Add ThermalState and ThermalGroupId to sim_core types**

- **Context:** Foundation types that all thermal modules will use.
- **Scope:** Add `ThermalState { temp_mk: u32, thermal_group: Option<ThermalGroupId> }` and `ThermalGroupId` type alias to `types.rs`. Add `thermal: Option<ThermalState>` to `ModuleState`. Add `#[serde(default)]` for backward compat.
- **Non-goals:** No tick behavior. No content changes.
- **Acceptance criteria:** Types compile. Existing tests pass unchanged. Serialization round-trips with and without thermal data.
- **Tests:** Unit test: serialize ModuleState with thermal=None, deserialize → None. Serialize with thermal=Some, round-trip. Existing test suite green.
- **Telemetry:** None.
- **Dependencies:** None.
- **Risk:** Low. Additive types only.

**H0-02: Add ThermalDef to ModuleDef behavior system**

- **Context:** Content-driven thermal properties for modules.
- **Scope:** Add `ThermalDef { heat_capacity_j_per_k, passive_cooling_coefficient, max_temp_mk, operating_min_mk, operating_max_mk, thermal_group }` to `types.rs`. Add optional `thermal: Option<ThermalDef>` to `ModuleDef`. Update `sim_world` content loading.
- **Non-goals:** No new module definitions yet.
- **Acceptance criteria:** Existing module_defs.json loads without thermal fields (all None). A test module def with thermal fields loads correctly.
- **Tests:** Unit: load existing content → all thermal=None. Unit: load test content with thermal fields → parsed correctly.
- **Telemetry:** None.
- **Dependencies:** H0-01.
- **Risk:** Low. Optional field on existing struct.

**H0-03: Add MaterialThermalProps and Phase to types**

- **Context:** Types for temperature-carrying material batches.
- **Scope:** Add `MaterialThermalProps { temp_mk, phase, latent_heat_buffer_j }`, `Phase { Solid, Liquid }` to `types.rs`. Add `thermal: Option<MaterialThermalProps>` to `InventoryItem::Material` variant. `#[serde(default)]` for compat.
- **Non-goals:** No phase transition logic. No melting points in content.
- **Acceptance criteria:** Existing inventory serialization unchanged. Material items with thermal=None behave identically to before. Material with thermal=Some serializes correctly.
- **Tests:** Serialize/deserialize round-trip tests. Existing refinery/assembler tests pass (thermal=None on all outputs).
- **Telemetry:** None.
- **Dependencies:** None (parallel with H0-01).
- **Risk:** Medium — modifying InventoryItem enum touches many files. Must ensure all match arms handle new field.

**H0-04: Add thermal constants to constants.json**

- **Context:** Tunable thermal parameters.
- **Scope:** Add to Constants struct and constants.json: `thermal_sink_temp_mk: u32` (293_000), `thermal_overheat_warning_offset_mk: u32` (200_000), `thermal_overheat_critical_offset_mk: u32` (500_000), `thermal_wear_multiplier_warning: f32` (2.0), `thermal_wear_multiplier_critical: f32` (4.0). Add to test_fixtures.
- **Non-goals:** No element thermal properties yet.
- **Acceptance criteria:** Constants load. Test fixtures updated. Existing tests pass.
- **Tests:** Unit: constants deserialize with new fields. Test fixtures include thermal constants.
- **Telemetry:** None.
- **Dependencies:** None.
- **Risk:** Low.

**H0-05: Add element thermal properties to elements.json schema**

- **Context:** Melting points and specific heats for phase transition math.
- **Scope:** Add optional fields to element definition: `melting_point_mk`, `latent_heat_j_per_kg`, `specific_heat_j_per_kg_k`. Update Fe: melting_point_mk=1_811_000, latent_heat=247_000, specific_heat=449. Other elements: None (not thermally relevant yet).
- **Non-goals:** No phase transition logic.
- **Acceptance criteria:** Elements load. Existing elements without thermal fields load as None.
- **Tests:** Unit: load elements, verify Fe has thermal props, ore has None.
- **Telemetry:** None.
- **Dependencies:** None.
- **Risk:** Low.

**H0-06: Add dt_s helper and energy↔thermal unit conversion**

- **Context:** Establish the canonical contract: Power in Watts, heat in Joules, conversion via dt_s.
- **Scope:** Add `pub fn dt_seconds(constants: &Constants) -> f64` returning `minutes_per_tick * 60.0` (if minutes_per_tick exists) or `60.0` (fallback). Add `pub fn power_to_heat_j(watts: f32, dt_s: f64) -> i64` and `pub fn heat_to_temp_delta_mk(heat_j: i64, capacity_j_per_k: f32) -> i32`. Place in new `crates/sim_core/src/thermal.rs` module.
- **Non-goals:** No tick integration.
- **Acceptance criteria:** Helper functions produce correct conversions. Integer arithmetic, no float drift.
- **Tests:** Unit: dt_s with minutes_per_tick=1.0 → 60.0. power_to_heat(100W, 60s) → 6000J. heat_to_temp_delta(6000J, 100 J/K) → 60_000 mk.
- **Telemetry:** None.
- **Dependencies:** None.
- **Risk:** Low. Pure functions.

---

#### Epic H1: Thermal Tick + Smelter MVP

**H1-01: Implement thermal tick step in station module loop**

- **Context:** Core thermal simulation — runs every tick for thermal modules.
- **Scope:** Add step 3.6 in `station/mod.rs` tick sequence. For each module with ThermalDef: compute passive cooling, apply temperature delta, clamp. Group modules by ThermalGroupId, process in sorted order.
- **Non-goals:** No heat generation from recipes (separate ticket). No radiators. No overheat damage.
- **Acceptance criteria:** Module temperatures update each tick. Cooling toward sink temp. Deterministic (same seed → same temps). Existing tests pass.
- **Tests:** Unit: module at 500K with cooling coeff, verify temp decreases toward sink over N ticks. Unit: module at sink temp, verify temp stable. Determinism: two runs, identical temp history.
- **Telemetry:** None yet.
- **Dependencies:** H0-01, H0-02, H0-04, H0-06.
- **Risk:** Medium — modifying tick loop ordering. Must not break existing module ticks.

**H1-02: Add recipe thermal requirements and temperature gating**

- **Context:** Recipes can require minimum temperature to run.
- **Scope:** Add optional `RecipeThermalReq` to `RecipeDef`. In processor tick, check if module temp meets recipe min_temp_mk. If not, skip (stall with reason TooCoold). If within range, apply yield scaling based on temp position. Add heat_per_run_j to module's thermal state when recipe executes.
- **Non-goals:** No new recipes yet. No quality degradation from overheating (separate ticket).
- **Acceptance criteria:** Processor with thermal recipe stalls when cold. Runs when temp >= min. Heat generated per run increases module temp.
- **Tests:** Unit: cold smelter (293K) with recipe requiring 1800K → stalls. Unit: hot smelter (1900K) → runs, generates heat. Unit: yield scaling between min and optimal.
- **Telemetry:** None yet.
- **Dependencies:** H1-01, H0-02.
- **Risk:** Medium — modifying processor tick logic.

**H1-03: Add Smelter module definition to content**

- **Context:** First thermal module — higher-yield Fe processing that requires heat.
- **Scope:** Add `module_basic_smelter` to module_defs.json. Processor behavior with ThermalDef. Recipe: 500kg ore → Fe (higher yield than cold refinery, ~85% extraction vs ~70%) + slag. Requires min 1_800_000 mk, optimal 1_850_000–1_950_000, max 2_100_000. Heat per run: +50_000_000 J (50 MJ). Power: 30 kW. Wear: 0.015. Heat capacity: 50_000 J/K. Passive cooling: 5.0 W/K.
- **Non-goals:** No new elements. Uses existing Fe output.
- **Acceptance criteria:** Module loads from content. Can be added to station. Requires heat to operate.
- **Tests:** Integration: add smelter to test state, verify it stalls when cold, runs when heated (via direct temp_mk injection in test).
- **Telemetry:** None yet.
- **Dependencies:** H1-02.
- **Risk:** Low. Content + existing Processor infrastructure.

**H1-04: Add Radiator module behavior type**

- **Context:** Primary heat sink. Cools modules in its thermal group.
- **Scope:** New `ModuleBehaviorDef::Radiator(RadiatorDef)` and `ModuleKindState::Radiator(RadiatorState)`. RadiatorDef: `cooling_capacity_w: f32`. RadiatorState: `ticks_since_last_run: u64`. In thermal tick step, sum radiator capacity per group, subtract from group's heat budget. Wear reduces cooling capacity via wear_efficiency.
- **Non-goals:** No targeted cooling (cools entire group evenly). No active power draw.
- **Acceptance criteria:** Radiator reduces module temperatures in its group. Multiple radiators stack. Wear reduces effectiveness.
- **Tests:** Unit: smelter at 2000K + radiator → temp decreases. Unit: smelter generating heat + radiator → reaches equilibrium. Unit: worn radiator → less cooling → higher equilibrium.
- **Telemetry:** None yet.
- **Dependencies:** H1-01, H0-02.
- **Risk:** Medium — new module behavior variant. Must add to all match arms.

**H1-05: Add Radiator module definition to content**

- **Context:** First radiator panel module.
- **Scope:** Add `module_basic_radiator` to module_defs.json. Radiator behavior. Cooling capacity: 500 W. Wear: 0.001 per tick (slow, it's passive). Mass: 800 kg. Volume: 15 m3 (radiators are large). Power: 0.
- **Non-goals:** No advanced radiator types.
- **Acceptance criteria:** Module loads. Can be installed. Cools modules in group.
- **Tests:** Integration: smelter + radiator → verify thermal equilibrium in range.
- **Telemetry:** None yet.
- **Dependencies:** H1-04.
- **Risk:** Low.

**H1-06: Add thermal modules to test_fixtures.rs**

- **Context:** Test infrastructure for all future thermal tests.
- **Scope:** Add `state_with_smelter()`, `state_with_radiator()`, `state_with_smelter_and_radiator()` to test fixtures. Include appropriate thermal constants. Add `smelter_content()` helper.
- **Non-goals:** No molten material fixtures (v1).
- **Acceptance criteria:** Fixture functions produce valid state. Can be used in thermal tests.
- **Tests:** Smoke test: each fixture produces loadable state.
- **Telemetry:** None.
- **Dependencies:** H1-03, H1-05.
- **Risk:** Low.

**H1-07: Update dev_base_state.json with smelter + radiator**

- **Context:** Running sim should include thermal modules.
- **Scope:** Add 1 smelter + 2 radiators to starting station in dev_base_state.json. Update build_initial_state(). Keep existing cold refinery — player has both paths (cold low-yield, hot high-yield).
- **Non-goals:** No autopilot thermal management yet.
- **Acceptance criteria:** Sim starts, smelter heats up, reaches operating temp, runs recipes, radiators maintain equilibrium.
- **Tests:** Integration: run 1000 ticks, verify smelter reaches equilibrium and produces Fe.
- **Telemetry:** None yet.
- **Dependencies:** H1-03, H1-05, VIO-88, VIO-89 (power enforcement).
- **Risk:** Medium — must balance power budget with new modules.

---

#### Epic H2: Overheat Escalation

**H2-01: Implement overheat warning and critical zones**

- **Context:** Graduated consequences for thermal mismanagement.
- **Scope:** In thermal tick step, after temp update, check against max_temp_mk + offsets. Warning zone: emit OverheatWarning event, set heat_wear_multiplier to 2.0. Critical zone: emit OverheatCritical, multiplier 4.0, auto-stall module. Use constants for thresholds.
- **Non-goals:** No damage zone (separate ticket). No material loss.
- **Acceptance criteria:** Module entering warning zone emits event, wear increases. Module entering critical auto-stalls. Cooling below threshold resumes operation.
- **Tests:** Unit: push module past max_temp → warning event + 2x wear. Unit: push past critical → auto-stall + 4x wear. Unit: cool below max → resumes.
- **Telemetry:** overheat_warning_count, overheat_critical_count counters.
- **Dependencies:** H1-01, H0-04.
- **Risk:** Low. Event-driven, follows existing stall/resume pattern.

**H2-02: Implement overheat damage zone**

- **Context:** Catastrophic overheating has costly but recoverable consequences.
- **Scope:** When temp exceeds max + 500K: wear jumps to critical band (0.8), emit OverheatDamage event. If module contains thermal inventory (crucible), spill risk: material mass reduced by 50%, converted to slag. Threshold-based, not RNG.
- **Non-goals:** No permanent module destruction. No cascade to adjacent modules.
- **Acceptance criteria:** Module reaching damage zone takes wear hit. Crucible with molten material loses material. Always recoverable with maintenance.
- **Tests:** Unit: module at damage temp → wear jumps to 0.8. Unit: crucible with 1000kg molten Fe at damage temp → 500kg lost as slag.
- **Telemetry:** overheat_damage_count, molten_spill_kg counters.
- **Dependencies:** H2-01, H6 (for crucible tests — can stub with direct state manipulation).
- **Risk:** Medium. Material loss needs careful inventory accounting.

**H2-03: Integrate heat-wear multiplier into existing wear system**

- **Context:** Overheating accelerates wear across all thermal modules.
- **Scope:** Modify `wear.rs` to accept optional heat_wear_multiplier. In station tick, compute multiplier from overheat zone, pass to wear accumulation. `effective_wear = base_wear * heat_multiplier`.
- **Non-goals:** No changes to wear bands or maintenance logic.
- **Acceptance criteria:** Module in warning zone wears 2x faster. Module in nominal zone wears at base rate. Maintenance still repairs at same rate (creates pressure).
- **Tests:** Unit: smelter in warning zone for N runs → wear = N * base * 2.0. Unit: smelter in nominal → wear = N * base * 1.0.
- **Telemetry:** heat_wear_multiplier_avg gauge.
- **Dependencies:** H2-01.
- **Risk:** Low. Multiplicative modifier on existing system.

---

#### Epic H3: Thermal Metrics + Alerts

**H3-01: Add thermal metrics to MetricsSnapshot**

- **Context:** Observable thermal state for balance tuning and alerts.
- **Scope:** Add fields to MetricsSnapshot: station_max_temp_mk, station_avg_temp_mk, overheat_warning_count, overheat_critical_count, radiator_utilization_pct, heat_wear_multiplier_avg, smelter_yield_efficiency. Compute in metrics collection.
- **Non-goals:** No per-module temp breakdown (too verbose for snapshot).
- **Acceptance criteria:** Metrics populated during sim runs. Appear in daemon digest. Appear in sim_bench CSV output.
- **Tests:** Unit: compute metrics with thermal modules → fields populated. Unit: no thermal modules → fields default to 0.
- **Telemetry:** This IS the telemetry ticket.
- **Dependencies:** H1-01, H2-01.
- **Risk:** Low.

**H3-02: Add thermal alert rules to daemon AlertEngine**

- **Context:** Proactive warnings for thermal problems.
- **Scope:** New alert rules: "Station overheating" when any module in warning+ zone for >5 consecutive ticks. "Smelter too cold" when smelter below operating temp for >20 ticks. "Radiator capacity low" when utilization >90%.
- **Non-goals:** No thermal-specific alert UI (uses existing alert infrastructure).
- **Acceptance criteria:** Alerts fire on threshold. Clear when resolved. Appear in SSE stream.
- **Tests:** Unit per rule: construct metrics history triggering condition → AlertRaised. Construct clearing condition → AlertCleared.
- **Telemetry:** Uses existing alert counters.
- **Dependencies:** H3-01.
- **Risk:** Low. Follows existing alert pattern.

**H3-03: Add thermal override support to sim_bench**

- **Context:** Balance testing thermal parameters via scenario overrides.
- **Scope:** Support dotted-key overrides for thermal constants: `thermal.sink_temp_mk`, `thermal.overheat_warning_offset_mk`, `module.smelter.*`, `module.radiator.*`.
- **Non-goals:** No new scenarios yet (separate ticket).
- **Acceptance criteria:** Override thermal constants in scenario JSON. Sim runs with modified values.
- **Tests:** Unit: override sink temp, verify lower cooling. Override radiator capacity, verify equilibrium changes.
- **Telemetry:** None.
- **Dependencies:** H3-01.
- **Risk:** Low.

---

#### Epic H4: FE Thermal UI

**H4-01: Add temperature readout to station module panel**

- **Context:** Players need to see module temperatures.
- **Scope:** In station module list, show temp_mk as Kelvin (with C toggle) for thermal modules. Color-code: blue (<operating), green (operating), yellow (warning), red (critical). Show "N/A" for non-thermal modules.
- **Non-goals:** No thermal overlay or graph. No detailed tooltip yet.
- **Acceptance criteria:** Temperature visible on smelter and radiator modules. Colors match overheat zones. Updates with SSE stream.
- **Tests:** Vitest: render module with thermal state → shows temp. Render without → shows N/A.
- **Telemetry:** None.
- **Dependencies:** H1-01 (thermal state in snapshot/events).
- **Risk:** Low. Additive UI.

**H4-02: Add thermal state badge to module cards**

- **Context:** Quick visual indicator of thermal health.
- **Scope:** Badge on module card: "COLD" (below operating), "HOT" (warning), "CRITICAL" (critical), "DAMAGE" (damage zone). Uses existing badge/chip component pattern.
- **Non-goals:** No animation. No hover detail.
- **Acceptance criteria:** Badge appears when module is in non-nominal thermal state. Disappears when nominal.
- **Tests:** Vitest: module in warning zone → shows HOT badge. Module in operating → no badge.
- **Telemetry:** None.
- **Dependencies:** H4-01.
- **Risk:** Low.

**H4-03: Add thermal alerts to StatusBar**

- **Context:** Overheat alerts visible in top-level status bar.
- **Scope:** Thermal alerts (from H3-02) appear in existing alert badge system. No new UI components — thermal alerts flow through existing AlertEngine → SSE → UI pipeline.
- **Non-goals:** No dedicated thermal alert filter (deferred).
- **Acceptance criteria:** Overheat alerts appear in status bar. Clicking navigates to station panel.
- **Tests:** Vitest: alert state includes thermal alert → badge shows.
- **Telemetry:** None.
- **Dependencies:** H3-02.
- **Risk:** Low. Uses existing infrastructure.

**H4-04: Sync TypeScript types with new Rust thermal types**

- **Context:** FE must understand ThermalState, MaterialThermalProps, Phase, new module types.
- **Scope:** Update `ui_web/src/types.ts`: add ThermalState, MaterialThermalProps, Phase, RadiatorState, ThermalLink interfaces. Update ModuleKindState union. Update InventoryItem Material variant. Update applyEvents for thermal events.
- **Non-goals:** No new UI components.
- **Acceptance criteria:** TypeScript types match Rust types. applyEvents handles OverheatWarning, OverheatCritical, OverheatDamage events. Existing events still processed correctly.
- **Tests:** Vitest: applyEvents with thermal events → state updated correctly. Existing applyEvents tests pass.
- **Telemetry:** None.
- **Dependencies:** H0-01, H0-03, H1-04.
- **Risk:** Medium. Type sync has historically caused bugs (VIO-78).

---

#### Epic H5: Port/Link Abstraction

**H5-01: Add ModulePort and PortDirection to module types**

- **Context:** Modules declare input/output ports for directed material flow.
- **Scope:** Add `ModulePort { id, direction, accepts }`, `PortDirection`, `PortFilter` to types.rs. Add optional `ports: Vec<ModulePort>` to `ModuleDef`. Smelter gets `molten_out` port. Casting mold gets `molten_in` port. Crucible gets both.
- **Non-goals:** No link storage. No transfer logic.
- **Acceptance criteria:** Module defs with ports load correctly. Ports serialize/deserialize.
- **Tests:** Unit: load module def with ports → parsed. Load without → empty vec.
- **Telemetry:** None.
- **Dependencies:** None (parallel with H0/H1).
- **Risk:** Low.

**H5-02: Add ThermalLink storage to StationState**

- **Context:** Stations track explicit connections between module ports.
- **Scope:** Add `thermal_links: Vec<ThermalLink>` to StationState with `#[serde(default)]`. Add `CreateThermalLink` and `RemoveThermalLink` commands. Validate: both modules exist on station, ports exist and are compatible (output→input), no duplicate links.
- **Non-goals:** No transfer logic. No automatic linking.
- **Acceptance criteria:** Links can be created/removed via commands. Invalid links rejected. Links persist in save/load.
- **Tests:** Unit: create valid link → stored. Create invalid (wrong direction) → rejected. Remove → gone. Serialize round-trip.
- **Telemetry:** None.
- **Dependencies:** H5-01.
- **Risk:** Low.

**H5-03: Implement TransferMolten command**

- **Context:** Move discrete material batches along links.
- **Scope:** New `Command::TransferMolten { link_id, kg }`. Resolves in apply_commands step. Validates: link exists, source module has material, material is liquid phase, destination has capacity. Moves batch with temperature preserved. If material cools below melting point during transfer → solidifies in place → emit PipeFreeze event, link blocked.
- **Non-goals:** No automatic transfers. No flow rates. No routing.
- **Acceptance criteria:** Molten Fe transferred from crucible to mold. Temperature preserved. Freeze detection works.
- **Tests:** Unit: transfer 500kg molten Fe → arrives at destination. Unit: transfer material at melting point - 1K → freezes → event emitted. Unit: transfer exceeding capacity → rejected.
- **Telemetry:** pipe_freeze_count counter.
- **Dependencies:** H5-02, H0-03.
- **Risk:** Medium. Inventory mutation across modules — must invalidate volume caches correctly.

---

#### Epic H6: Molten Materials

**H6-01: Implement phase transition logic**

- **Context:** Materials change phase (solid↔liquid) based on temperature vs melting point.
- **Scope:** Add `fn update_phase(props: &mut MaterialThermalProps, element: &ElementDef, heat_delta_j: i64)` to thermal.rs. Implements latent heat buffer: when crossing melting point, heat goes into buffer instead of changing temp. Phase flips when buffer filled/drained. Hysteresis: solidification at melting_point - 50_000 mk (50K below) to prevent oscillation.
- **Non-goals:** No gas phase. No pressure.
- **Acceptance criteria:** Fe at 1811K + heat → latent buffer fills → phase=Liquid, temp resumes rising. Liquid Fe cooled → latent buffer drains → phase=Solid at 1761K.
- **Tests:** Unit: heat solid Fe through melting point → phase transition at correct temp. Unit: cool liquid Fe → solidifies with hysteresis. Unit: partial latent heat → phase holds. Unit: determinism across runs.
- **Telemetry:** None (phase tracked on item).
- **Dependencies:** H0-03, H0-05, H0-06.
- **Risk:** Medium. Latent heat math must be precise in integer arithmetic.

**H6-02: Add Crucible module behavior type**

- **Context:** Container that holds molten material at temperature.
- **Scope:** New `ModuleBehaviorDef::ThermalContainer(ThermalContainerDef)`. Holds inventory items with MaterialThermalProps. Has ThermalDef (insulated — low cooling coefficient). Has ports (molten_in, molten_out). Capacity in kg. Each tick: apply container cooling to held material, update phase.
- **Non-goals:** No automatic filling. Player/autopilot commands only.
- **Acceptance criteria:** Crucible holds molten Fe. Temperature decreases slowly (insulated). Phase transitions if cooled too much.
- **Tests:** Unit: crucible with 500kg liquid Fe at 1900K → cools slowly. Unit: crucible material → eventually solidifies if no heat source. Unit: capacity limit enforced.
- **Telemetry:** None.
- **Dependencies:** H6-01, H5-01.
- **Risk:** Medium. New inventory location (module-held vs station-held).

**H6-03: Add Casting Mold module — molten input to solid output**

- **Context:** Produces high-quality solid components from molten material.
- **Scope:** Processor variant with ThermalDef. Recipe: X kg Molten Fe → Cast Fe Part (component). Input filter: Material with phase=Liquid. Output: component with quality based on input temperature control. Exothermic: releases heat_per_run as material solidifies. Input consumed from connected crucible via link.
- **Non-goals:** No complex mold shapes. Single recipe per mold.
- **Acceptance criteria:** Mold accepts molten Fe from linked crucible. Produces Cast Fe Part component. Releases heat on casting.
- **Tests:** Unit: mold with liquid Fe input → produces component. Unit: mold with solid Fe → rejects (wrong phase). Unit: quality based on input temp within optimal range.
- **Telemetry:** smelter_yield_efficiency gauge (reused for casting).
- **Dependencies:** H6-02, H5-03, H1-02.
- **Risk:** Medium. New recipe input filter for phase state.

**H6-04: Add Cast Fe Part component and recipes to content**

- **Context:** New component produced by casting line, higher quality than assembled repair kit.
- **Scope:** Add `cast_fe_part` to component_defs.json. Mass: 30 kg. Volume: 0.05 m3. Add smelter recipe: 500kg ore → Molten Fe (material with thermal=Some, phase=Liquid). Add casting recipe: 200kg Molten Fe → 1 Cast Fe Part (quality from temp control). Add advanced assembler recipe: 2 Cast Fe Part + 100kg Fe → 1 Advanced Repair Kit (quality 1.0, more effective).
- **Non-goals:** No new repair mechanics. Advanced Repair Kit functionally identical to Repair Kit for now.
- **Acceptance criteria:** Full chain works: ore → smelter → crucible → mold → cast part → assembler → advanced kit.
- **Tests:** Integration: run full casting line for 100 ticks → produces advanced repair kits.
- **Telemetry:** None.
- **Dependencies:** H6-03.
- **Risk:** Low. Content changes.

**H6-05: Add Crucible, Casting Mold, Smelter to pricing.json**

- **Context:** Thermal modules importable for economy system.
- **Scope:** Add pricing entries for module_basic_smelter, module_basic_radiator, module_crucible, module_casting_mold. Price them high enough to incentivize research/manufacturing but available for import.
- **Non-goals:** No dynamic pricing.
- **Acceptance criteria:** Modules importable/exportable. Prices reasonable relative to existing modules.
- **Tests:** Integration: import smelter via trade → appears in inventory.
- **Telemetry:** None.
- **Dependencies:** H1-03, H6-02, H6-03.
- **Risk:** Low.

---

#### Epic H7: Testing & Benchmarks

**H7-01: Thermal determinism integration test**

- **Context:** Thermal sim must be perfectly deterministic.
- **Scope:** Test: run 1000 ticks with smelter+radiator+crucible, same seed → identical temp history, phase transitions, events. Compare full state equality.
- **Non-goals:** Not a performance test.
- **Acceptance criteria:** Two identical runs produce byte-for-byte identical state.
- **Tests:** This IS the test.
- **Dependencies:** H1-01, H6-01.
- **Risk:** Low.

**H7-02: Add thermal sim_bench scenario (30-day)**

- **Context:** Balance validation for thermal parameters.
- **Scope:** New scenario: `scenarios/thermal_30d.json`. Station with smelter, 2 radiators, crucible. Run 30 sim-days. Verify: smelter reaches equilibrium, no overheat events in steady state, Fe production rate reasonable.
- **Non-goals:** No multi-seed parallel runs (yet).
- **Acceptance criteria:** Scenario runs to completion. No crashes. Metrics in expected ranges.
- **Tests:** CI: scenario completes with zero collapses.
- **Dependencies:** H3-03.
- **Risk:** Low.

**H7-03: Add thermal sim_bench scenario (90-day with overheating)**

- **Context:** Verify overheat recovery and long-term thermal stability.
- **Scope:** New scenario: `scenarios/thermal_stress_90d.json`. Station with smelter + only 1 radiator (insufficient cooling). Override radiator capacity low. Verify: module enters warning zone, wear accelerates, eventually needs maintenance, recovers with repair kits.
- **Non-goals:** Not testing damage zone (separate, more risky).
- **Acceptance criteria:** Overheat events occur. Module enters degraded wear band from accelerated wear. Maintenance repairs. Production continues.
- **Tests:** CI: scenario completes. Overheat metrics > 0. No crashes.
- **Dependencies:** H7-02, H2-01.
- **Risk:** Low.

**H7-04: Regression test — existing refinery unchanged by thermal system**

- **Context:** Cold refinery must work identically to before.
- **Scope:** Test: run existing refinery tests. Verify: no thermal state on cold refinery modules. Same output, same wear, same events. No behavioral change to non-thermal modules.
- **Non-goals:** Not testing thermal modules.
- **Acceptance criteria:** All existing refinery, assembler, wear, maintenance tests pass without modification.
- **Tests:** This IS the regression test. Run full existing test suite.
- **Dependencies:** H1-01.
- **Risk:** Low but critical.

---

#### Epic H8: Documentation

**H8-01: Update docs/reference.md with thermal system**

- **Context:** Reference doc must cover new types, tick ordering, and module behaviors.
- **Scope:** Add sections: thermal types, thermal tick step (3.6), smelter/radiator/crucible/mold module docs, port/link docs, overheat escalation table, thermal constants.
- **Dependencies:** H2-01, H5-03, H6-03.

**H8-02: Update CLAUDE.md with thermal system**

- **Context:** AI assistant needs to know about thermal system.
- **Scope:** Update tick order documentation. Add thermal module types to architecture section. Add thermal test commands.
- **Dependencies:** H8-01.

**H8-03: Write thermal system design doc (persist this document)**

- **Context:** This design document should be saved as the canonical reference.
- **Scope:** Save this plan to `docs/heat_and_molten_materials_design.md` as permanent design reference. Strip implementation details, keep architecture and decisions.
- **Dependencies:** None.

---

## Dependency Graph

```
H0-01 ──┬──→ H1-01 ──→ H1-02 ──→ H1-03 ──→ H1-06 ──→ H1-07
H0-02 ──┘      │         │                      │
H0-04 ─────────┘         │                      │
H0-06 ────────────────────┤                      │
                          │                      │
H0-03 ──→ H6-01 ──→ H6-02 ──→ H6-03 ──→ H6-04  │
H0-05 ──┘                                       │
                                                 │
H5-01 ──→ H5-02 ──→ H5-03 ──────────────────────┤
                                                 │
H1-04 ──→ H1-05 ────────────────────────────────→│
                                                 │
H2-01 ──→ H2-02                                  │
  │──→ H2-03                                     │
  │──→ H3-01 ──→ H3-02 ──→ H3-03                │
                                                 │
H4-04 ──→ H4-01 ──→ H4-02                       │
                 ──→ H4-03                       │
                                                 │
H7-01, H7-02, H7-03, H7-04 ─── (after respective epics)
H8-01, H8-02, H8-03 ─── (after all epics)

EXTERNAL PREREQUISITE:
VIO-88 + VIO-89 (power enforcement) ──→ H1-07 (dev_base_state with power)
```

## Parallelism Opportunities

- **H0-01 through H0-06** can all be developed in parallel (independent type additions)
- **H5-01 through H5-02** can run in parallel with **H1-01 through H1-05** (port types vs tick logic)
- **H6-01** (phase transitions) can start as soon as H0-03 + H0-05 land, parallel with H1 epic
- **H4-04** (TS type sync) can start as soon as Rust types are stable (after H0)
- **H8-03** (save design doc) can happen immediately
