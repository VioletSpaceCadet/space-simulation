# Energy & Propellant System Design

> Status: Design phase. No code changes yet.
> Companion docs: `solar_system_abstraction.md`, `movement_cost_model.md`

## 1. High-Level Goals

Transform the simulation from "produce iron and ships" to "manage energy, fuel, logistics, and infrastructure sustainability."

Energy and propellant are **systemic constraints** — they interact with every existing system (mining, refining, manufacturing, research, maintenance, storage) and create new strategic tradeoffs at every scale.

### What Changes for the Player

| Before | After |
|---|---|
| Modules run whenever enabled | Modules compete for limited power budget |
| Ships travel for free | Ships consume propellant; stranded ships are idle |
| All asteroids are metal-rich | Volatile-rich asteroids provide water/ice for propellant |
| Location choice is "which belt zone" | Location determines solar intensity, resource type, travel cost |
| Scaling = more modules | Scaling = more modules + power infrastructure + fuel logistics |

### Design Spine Alignment

- **Creates strategic tradeoffs**: Inner belt (high solar, metal-rich) vs outer belt (low solar, volatile-rich). Power vs throughput. Propellant reserves vs cargo space.
- **Reinforces systemic interactions**: Energy shortage → module stalling → production slowdown → maintenance backlog. Propellant shortage → fleet idle → ore starvation → refinery starved.
- **Produces emergent outcomes**: Player discovers they need a volatile-mining outpost to fuel their metal-mining fleet.
- **Observable through metrics**: Power deficit %, propellant reserves, boil-off rate, fleet range.
- **Counterable by infrastructure**: Solar arrays, batteries, electrolysis plants, cryogenic storage.

---

## 2. System Overview

### 2.1 Energy System

**Model:** Per-tick power budget at each station.

```
power_generated (solar arrays, etc.)
  + power_discharged (batteries)
  - power_consumed (modules running this tick)
  = power_surplus (charges batteries) or power_deficit (modules stall)
```

**Key properties:**
- Power is **not an inventory item**. It is generated, consumed, and optionally stored each tick.
- Solar arrays generate power proportional to `solar_intensity` at their station's location.
- Batteries store surplus power and discharge during deficit.
- When power is insufficient, modules are **stalled** in priority order (lowest priority first). Existing stall/resume event infrastructure handles this.
- Power generation modules accumulate wear like any other module.

**Why not inventory-based energy?** Energy-per-tick is simpler, more intuitive, and avoids volume/mass accounting for something that doesn't physically sit in storage. Batteries are the exception — they are modules with internal state, not inventory items.

### 2.2 Water Extraction & Propellant Production

**New production chain:**

```
Volatile-rich asteroid
  → Mine → Ice-bearing ore (existing mining system, new ore type)
  → Heating Module (Processor) → Water + Slag
    [Energy cost: moderate]
  → Electrolysis Module (Processor) → LH2 + LOX
    [Energy cost: high]
  → Cryogenic Tank (inventory) → Ship propellant tank
    [Boil-off: passive loss over time]
```

**Key properties:**
- Water (H2O) is a new element in `elements.json` with its own density.
- LH2 (liquid hydrogen) and LOX (liquid oxygen) are new elements. LH2 is the primary propellant.
- Heating and electrolysis are Processor modules with recipes — they fit the existing module architecture.
- Boil-off is a new per-tick passive loss on cryogenic inventory items, analogous to wear on modules. Rate depends on storage quality (insulated tanks reduce loss).
- LOX is a byproduct of electrolysis — useful for future life support or as oxidizer, but initially just stored/vented.

### 2.3 Propellant-Based Movement

**Model:** Ships consume LH2 propellant for each transit hop.

```
propellant_consumed_kg = hop_delta_v * (ship_dry_mass + cargo_mass + propellant_mass) / exhaust_velocity
```

See `movement_cost_model.md` for full specification.

**Key properties:**
- Ships have a `propellant_capacity_kg` and `propellant_kg` in their state.
- Propellant is loaded at stations (new Refuel task or automatic on deposit).
- Ships cannot transit if insufficient propellant for the trip.
- Autopilot must account for return-trip propellant when deciding whether to transit.
- Propellant mass counts toward ship total mass, creating the classic rocket equation tradeoff.

---

## 3. Architectural Impact

### 3.1 sim_core Changes

**New types:**
- `PowerState` on `StationState` — tracks generation, consumption, battery level
- `BatteryState` on battery modules — charge level, max capacity, charge/discharge rates
- `BoiloffState` on cryogenic inventory items — tracks passive loss
- `propellant_capacity_kg` and `propellant_kg` on `ShipState`
- New `InventoryItem` variants or new elements: `H2O`, `LH2`, `LOX`
- `TaskKind::Refuel` — ship loads propellant at station

**Tick loop changes:**
- **New step 3.0 (before modules):** Compute power budget. Solar generation based on location. Sum module power demands. Stall lowest-priority modules if deficit.
- **New step 3.5 (after maintenance):** Apply boil-off to cryogenic inventory items.
- **Modify step 2 (resolve ship tasks):** Transit resolution deducts propellant. Transit creation validates propellant sufficiency.

**New module behaviors:**
- `SolarArray` — generates power based on `solar_intensity` at station location. No interval (continuous). Wear reduces output.
- `Battery` — charges from surplus, discharges during deficit. Wear reduces capacity.
- `HeatingModule` — Processor variant. Recipe: Ice ore → H2O + slag. Energy cost per run.
- `ElectrolysisModule` — Processor variant. Recipe: H2O → LH2 + LOX. High energy cost.

**Existing module changes:**
- All modules now actually enforce `power_consumption_per_run`. Currently this field exists but is not checked. It becomes the real power draw.
- Module stalling gains a new cause: `StallReason::PowerDeficit` alongside existing `StallReason::StorageFull`.

### 3.2 sim_control Changes

- Autopilot must check propellant before issuing transit commands.
- Autopilot should auto-refuel ships when docked at a station with LH2.
- New heuristic: if propellant is low and no LH2 on station, prioritize electrolysis/heating.
- Power management: autopilot may need to disable low-priority modules when power is constrained (or this could be automatic via the power budget system).

### 3.3 sim_daemon Changes

- New metrics exposed: power_generated, power_consumed, power_deficit, battery_level, propellant_reserves, boiloff_rate.
- New alert rules: low propellant, power deficit, high boil-off.
- No structural changes to SSE or endpoint architecture.

### 3.4 Frontend Changes

- Power bar/gauge in station panel (generation vs consumption).
- Propellant indicator on ship cards.
- New module types visible in station module list.
- Resource flow visualization (optional, later).

### 3.5 Content Changes

- `elements.json` — add H2O, LH2, LOX with densities
- `module_defs.json` — add solar array, battery, heating module, electrolysis module
- `asteroid_templates.json` — add volatile-rich templates with H2O composition
- `solar_system.json` — add `solar_intensity` per node (see `solar_system_abstraction.md`)
- `techs.json` — add energy and propellant techs
- `constants.json` — add power, propellant, boil-off constants
- `component_defs.json` — add solar panel, battery cell, cryo tank components if needed

---

## 4. Determinism Considerations

All new systems must maintain deterministic behavior:

- **Power budget** is computed from state each tick — no floating accumulation drift. Battery charge is clamped to `[0, max]` after each tick.
- **Boil-off** uses fixed rates per tick, not real-time durations. `boiloff_per_tick = rate * kg`. Applied after module ticks, before tick increment.
- **Propellant consumption** is computed at transit start and deducted atomically. No mid-transit consumption (ship is "in warp" for `total_ticks`).
- **Module stalling order** is deterministic: sorted by (priority, module_id). Priority is a new field on ModuleDef or derived from module type.
- **Solar intensity** is a static property of each node — no orbital variation.

---

## 5. Production Graph Specification

### 5.1 Resource Nodes

| Resource | Type | Density (kg/m³) | Notes |
|---|---|---|---|
| Ore (metal) | Raw | 3,000 | Existing. Fe/Si rich. |
| Ore (volatile) | Raw | 1,500 | New. H2O/He rich. Lower density (ice). |
| Slag | Waste | 2,500 | Existing. |
| Fe | Refined | 7,874 | Existing. |
| Si | Refined | 2,329 | Existing. |
| H2O | Intermediate | 1,000 | New. Extracted from volatile ore. |
| LH2 | Propellant | 71 | New. Very low density — takes lots of volume. |
| LOX | Oxidizer | 1,141 | New. Byproduct of electrolysis. |
| He | Refined | 125 | Existing. Future use. |

### 5.2 Transformation Edges

```
Metal-rich Ore ──[Iron Refinery, 10 kW]──→ Fe + Slag
Metal-rich Ore ──[Silicon Refinery, 12 kW]──→ Si + Slag     (future)
Volatile Ore ──[Heating Module, 15 kW]──→ H2O + Slag
H2O ──[Electrolysis Module, 25 kW]──→ LH2 + LOX
Fe ──[Assembler, 8 kW]──→ Repair Kit
Fe + Thruster ──[Shipyard, 25 kW]──→ Ship                    (existing)
LH2 ──[Ship Refuel]──→ Ship Propellant Tank                  (new task)
```

### 5.3 Energy Flows

```
Solar Array ──[solar_intensity × base_output × wear_efficiency]──→ Power Budget
Battery ──[discharge_rate]──→ Power Budget (when deficit)
Power Budget ──[charge_rate]──→ Battery (when surplus)
Power Budget ──[consumption]──→ All Active Modules
```

### 5.4 Storage Considerations

| Resource | Storage Type | Special |
|---|---|---|
| Ore, Fe, Si, Slag | Station inventory | Volume-limited (existing) |
| H2O | Station inventory | Volume-limited, no special handling |
| LH2 | Station inventory (cryo) | Volume-limited + boil-off |
| LOX | Station inventory (cryo) | Volume-limited + boil-off (lower rate) |
| Power | Not stored directly | Generated and consumed per-tick |
| Battery charge | Module internal state | Not inventory; kWh capacity on BatteryState |

### 5.5 Boil-off Model

Cryogenic resources (LH2, LOX) lose mass each tick:

```
loss_kg = boiloff_rate_per_tick * current_kg
```

Default rates (tunable via constants):
- LH2: 0.001% per tick (0.06% per hour, ~1.4% per day) — significant pressure
- LOX: 0.0002% per tick (~0.3% per day) — slower, denser, easier to keep cold

Insulated cryo tank tech reduces boil-off rate by 50–75%.

### 5.6 Degradation Interactions

| System | Wear Effect |
|---|---|
| Solar Array | Reduced power output (existing wear_efficiency bands) |
| Battery | Reduced max capacity (charge stored capped at `max * efficiency`) |
| Heating Module | Reduced H2O yield (existing processor wear logic) |
| Electrolysis Module | Reduced LH2 yield (existing processor wear logic) |
| Ship engines (future) | Increased propellant consumption per hop |

---

## 6. Minimal Viable First Implementation (MVP)

The MVP is the smallest slice that introduces energy as a real constraint:

1. **Solar arrays** as a new module type that generates power
2. **Power budget enforcement** — modules stall when insufficient power
3. **`station_power_available_per_tick`** becomes computed from solar arrays rather than a constant
4. One new metric: `power_deficit_count`

This is **Epic 1** in the implementation plan. Everything else builds on it.

---

## 7. Explicitly Out of Scope

| Topic | Reason |
|---|---|
| Nuclear reactors | Later power source. Solar first for simplicity. |
| Life support / oxygen consumption | Not an entropy system yet. LOX is stored but unused. |
| Orbital mechanics | Design spine explicitly prohibits this. |
| Real thermodynamics | Boil-off is a flat rate, not thermal modeling. |
| Multi-station power grids | Each station has its own independent power budget. |
| Dynamic pricing for propellant | Economy system exists but propellant pricing is a future concern. |
| Ship-to-ship propellant transfer | Ships refuel at stations only. |
| Ion drives / electric propulsion | Future tech. Start with chemical (LH2/LOX). |
| Asteroid orbital positions | Asteroids are at fixed nodes. No orbital periods. |
| Solar panel degradation from radiation | Handled by existing wear system — no special radiation model. |

---

## 8. Open Questions (Deferred to Implementation)

1. **Should power priority be configurable?** Module stalling order when power is short. Probably: maintenance > refinery > assembler > labs > sensors. Configurable later.
2. **Should ships carry solar panels?** For now, no. Ships refuel at stations. Ship power is implicit.
3. **Should LOX be required for propulsion?** Simplest model: LH2 only. Realistic model: LH2 + LOX as bipropellant. Start simple, add LOX requirement as a tech upgrade.
4. **How does propellant interact with the economy?** LH2 should be importable/exportable via pricing.json. Deferred to after core implementation.
