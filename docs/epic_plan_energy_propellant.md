# Epic Plan: Energy & Propellant Systems

> 5 epics, ~50 tickets total. Each epic is independently mergeable.
> Dependency order: Epic 1 & 2 (parallel) → Epic 3 → Epic 4. Epic 5 incremental throughout.

---

## Epic 1: Energy System Foundation

**Goal:** Make power a real constraint. Solar arrays generate power. Modules enforce consumption. Batteries buffer.

**Depends on:** Nothing (start immediately)
**Blocks:** Epic 3, Epic 5

### Tickets

#### E1-01: Add solar_intensity to NodeDef and solar_system.json
- **Crates:** sim_core (types), sim_world
- **Changes:** Add `solar_intensity: f32` field to `NodeDef` (default 1.0). Update `solar_system.json` with per-node values. Update `GameContent` loading.
- **Schema:** `NodeDef { ..., solar_intensity: f32 }`
- **Bench:** No impact yet (field is passive)
- **FE:** None

#### E1-02: Add SolarArray module behavior type
- **Crates:** sim_core (types, station)
- **Changes:** New `ModuleKindState::SolarArray(SolarArrayState)`. New `SolarArrayDef` in module behavior. `SolarArrayState { ticks_since_last_run: u64 }`. Tick function computes `base_output * solar_intensity * wear_efficiency`. Returns power generated.
- **Schema:** New variant on `ModuleKindState`, new behavior type in `ModuleDef`
- **Bench:** None yet
- **FE:** None

#### E1-03: Add solar array module definition to content
- **Crates:** None (content only)
- **Changes:** Add `module_basic_solar_array` to `module_defs.json`. Base output: 50 kW. Wear: 0.002 per run. Power consumption: 0 (it generates power).
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E1-04: Add PowerState to StationState
- **Crates:** sim_core (types)
- **Changes:** Add `PowerState { generated_kw: f32, consumed_kw: f32, deficit_kw: f32 }` to `StationState`. Computed fresh each tick, not persisted across ticks (derived state).
- **Schema:** `StationState { ..., power: PowerState }`
- **Bench:** None
- **FE:** None

#### E1-05: Implement power budget computation in tick loop
- **Crates:** sim_core (engine, station)
- **Changes:** New step 3.0 in tick loop: iterate solar arrays, sum power generated (applying solar_intensity and wear_efficiency). Sum power_consumption_per_run of all enabled modules that will run this tick. Compute deficit. Store in `PowerState`.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E1-06: Enforce power consumption — stall modules on deficit
- **Crates:** sim_core (station, types)
- **Changes:** When power deficit > 0, stall lowest-priority modules first until budget balances. Add `ModuleStallReason::PowerDeficit` to stall events. Priority order (highest first): Maintenance > Processor > Assembler > Lab > SensorArray. Use existing stall/resume event infrastructure.
- **Schema:** New enum variant on stall reason
- **Bench:** None
- **FE:** None

#### E1-07: Add Battery module behavior type
- **Crates:** sim_core (types, station)
- **Changes:** `ModuleKindState::Battery(BatteryState)`. `BatteryState { charge_kw: f32, max_capacity_kw: f32 }`. Charges from surplus, discharges during deficit. Wear reduces max capacity. New `BatteryDef` with charge_rate and discharge_rate.
- **Schema:** New variant on `ModuleKindState`
- **Bench:** None
- **FE:** None

#### E1-08: Add battery and solar array to dev_base_state.json
- **Crates:** sim_world, content
- **Changes:** Add `module_basic_solar_array` (x2) and `module_basic_battery` to station inventory in `dev_base_state.json`. Update `build_initial_state()` to match. Set `station_power_available_per_tick` to 0 (power now computed from solar arrays, not constant).
- **Schema:** None
- **Bench:** Baseline scenario will now show power metrics
- **FE:** None

#### E1-09: Add power metrics to MetricsSnapshot
- **Crates:** sim_core (metrics)
- **Changes:** Add `power_generated_kw`, `power_consumed_kw`, `power_deficit_kw`, `battery_charge_pct` to `MetricsSnapshot`. Add to `compute_metrics()`.
- **Schema:** New fields on MetricsSnapshot
- **Bench:** New columns in CSV, new fields in batch_summary aggregation
- **FE:** None

#### E1-10: Add power override support to sim_bench
- **Crates:** sim_bench (overrides)
- **Changes:** Support `module.solar_array.base_output_kw` and `module.battery.*` overrides.
- **Schema:** None
- **Bench:** Enables power balance testing via scenario overrides
- **FE:** None

#### E1-11: Run baseline + month scenarios, verify power balance
- **Crates:** None (verification only)
- **Changes:** Run scenarios with energy system active. Verify no collapses, power metrics reasonable, modules stall correctly when solar arrays removed via override.
- **Schema:** None
- **Bench:** Update baseline expectations
- **FE:** None

#### E1-12: FE — power bar in station panel
- **Crates:** ui_web
- **Changes:** Display power_generated vs power_consumed as a bar/gauge in station info. Show deficit warning. Show battery charge level.
- **Schema:** None
- **Bench:** None
- **FE:** New component in station panel

---

## Epic 2: Asteroid Resource Typing

**Goal:** Volatile-rich asteroids with H2O. Expanded solar system graph. Variable travel time. Heating module extracts water.

**Depends on:** Nothing (can run in parallel with Epic 1)
**Blocks:** Epic 3, Epic 4

### Tickets

#### E2-01: Add resource_class to NodeDef
- **Crates:** sim_core (types), sim_world
- **Changes:** Add `resource_class: Option<ResourceClass>` to `NodeDef`. Enum: `MetalRich`, `Mixed`, `VolatileRich`. Default `None` (backward compatible). Update content loading.
- **Schema:** New field on NodeDef, new enum
- **Bench:** None
- **FE:** None

#### E2-02: Add volatile-rich and carbonaceous asteroid templates
- **Crates:** None (content only)
- **Changes:** Add `tmpl_volatile_rich` and `tmpl_carbonaceous` to `asteroid_templates.json` with H2O composition ranges. Add `VolatileRich` and `Carbonaceous` anomaly tags.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E2-03: Add H2O element to elements.json
- **Crates:** None (content only)
- **Changes:** Add `{ "id": "H2O", "density_kg_per_m3": 1000.0, "display_name": "Water Ice", "refined_name": "Water" }` to elements.json.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E2-04: Template selection based on node resource_class
- **Crates:** sim_world (or sim_core scan site generation)
- **Changes:** When generating scan sites for a node, filter/weight asteroid templates based on the node's `resource_class`. MetalRich → 80% iron_rich + 20% silicate. VolatileRich → 60% volatile_rich + 30% carbonaceous + 10% silicate. Mixed → balanced. Null → current behavior (all templates equal).
- **Schema:** None
- **Bench:** Run scenarios to verify asteroid type distribution
- **FE:** None

#### E2-05: Expand solar_system.json with new nodes and hop_dv edges
- **Crates:** None (content only)
- **Changes:** Add `node_lunar_orbit` and `node_trojan` nodes. Add `hop_dv` to all edges. Add `scan_site_weight` per node.
- **Schema:** Edge format changes from `[id, id]` to `{ from, to, hop_dv }`
- **Bench:** None
- **FE:** None

#### E2-06: Variable travel time from hop_dv
- **Crates:** sim_core (tasks, graph)
- **Changes:** When computing transit duration, use `edge.hop_dv * ticks_per_dv_unit` instead of flat `travel_ticks_per_hop`. Add `ticks_per_dv_unit` constant. Fallback: if edge has no `hop_dv`, use `travel_ticks_per_hop`.
- **Schema:** New constant
- **Bench:** Travel time now varies by route
- **FE:** None (transit duration already displayed from task state)

#### E2-07: Heating module — Processor for ice ore → water
- **Crates:** sim_core (content only — it's a standard Processor)
- **Changes:** Add `module_heating_unit` to `module_defs.json`. Processor behavior with recipe: 1,000 kg ore (ice) → H2O (yield based on H2O fraction) + slag. Power consumption: 15 kW. Wear: 0.01.
- **Schema:** None (uses existing Processor infrastructure)
- **Bench:** None
- **FE:** None

#### E2-08: Update dev_base_state.json with expanded solar system
- **Crates:** sim_world, content
- **Changes:** Update `dev_base_state.json` scan sites to use new nodes. Distribute sites across nodes based on `scan_site_weight`. Update `build_initial_state()` to match.
- **Schema:** None
- **Bench:** Rerun baselines
- **FE:** None

#### E2-09: Add asteroid type tag to FE asteroid panel
- **Crates:** ui_web
- **Changes:** Display anomaly tags (IronRich, VolatileRich, Carbonaceous) in asteroid list. Color-code by type.
- **Schema:** None
- **Bench:** None
- **FE:** Tag badges in asteroid panel

#### E2-10: Autopilot — target volatile asteroids when water needed
- **Crates:** sim_control
- **Changes:** When station has a heating module and no H2O in inventory, prioritize mining volatile-rich asteroids. New autopilot heuristic alongside existing iron-rich targeting.
- **Schema:** None
- **Bench:** None
- **FE:** None

---

## Epic 3: Water to Propellant Chain

**Goal:** Electrolysis splits H2O → LH2 + LOX. Cryogenic storage with boil-off.

**Depends on:** Epic 1 (power enforcement), Epic 2 (H2O extraction)
**Blocks:** Epic 4

### Tickets

#### E3-01: Add LH2 and LOX elements
- **Crates:** None (content only)
- **Changes:** Add `{ "id": "LH2", "density_kg_per_m3": 71.0, "display_name": "Liquid Hydrogen" }` and `{ "id": "LOX", "density_kg_per_m3": 1141.0, "display_name": "Liquid Oxygen" }` to elements.json.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E3-02: Electrolysis module definition
- **Crates:** None (content only)
- **Changes:** Add `module_electrolysis_unit` to `module_defs.json`. Processor behavior with recipe: 1,000 kg H2O → ~111 kg LH2 + ~889 kg LOX (stoichiometric H2O split). Power: 25 kW (high energy cost). Wear: 0.012.
- **Schema:** None (uses existing Processor)
- **Bench:** None
- **FE:** None

#### E3-03: Material filter for H2O input
- **Crates:** sim_core (station)
- **Changes:** Verify that Processor recipe input filter `Element("H2O")` correctly matches `InventoryItem::Material { element: "H2O", .. }` in the existing FIFO consumption logic. The heating module produces Material items with element "H2O", and electrolysis consumes them. May need to verify InputFilter::Element matching for non-ore materials.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E3-04: Add boiloff_rate to element definitions
- **Crates:** sim_core (types), sim_world
- **Changes:** Add optional `boiloff_rate_per_tick: Option<f32>` to element definitions. LH2: 0.00001 (0.001% per tick), LOX: 0.000002. Other elements: None (no boil-off).
- **Schema:** New field on element definition
- **Bench:** None
- **FE:** None

#### E3-05: Implement boil-off in tick loop
- **Crates:** sim_core (engine or station)
- **Changes:** New step 3.5 in tick loop (after maintenance, before research): iterate all station inventory items of type Material. If the element has a boiloff_rate, reduce kg by `rate * kg`. Remove items that reach 0. Emit `BoiloffLoss { element, kg_lost }` event.
- **Schema:** New event type
- **Bench:** None
- **FE:** None

#### E3-06: Add boil-off metrics
- **Crates:** sim_core (metrics)
- **Changes:** Add `total_lh2_kg`, `total_lox_kg`, `boiloff_kg_per_tick` to MetricsSnapshot.
- **Schema:** New metric fields
- **Bench:** New CSV columns
- **FE:** None

#### E3-07: Add electrolysis and heating modules to dev_base_state.json
- **Crates:** sim_world, content
- **Changes:** Add heating unit and electrolysis unit to station inventory. Update `build_initial_state()`.
- **Schema:** None
- **Bench:** Rerun baselines
- **FE:** None

#### E3-08: Autopilot — manage water→propellant pipeline
- **Crates:** sim_control
- **Changes:** When station has electrolysis module and LH2 is below threshold, ensure heating module is enabled and H2O is being produced. Auto-enable/disable water extraction pipeline based on demand.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E3-09: Boil-off alert rules
- **Crates:** sim_daemon (alerts)
- **Changes:** Add alert rules: "High LH2 boil-off" when boiloff_kg_per_tick exceeds threshold. "Low LH2 reserves" when total_lh2_kg below threshold.
- **Schema:** None
- **Bench:** None
- **FE:** Alert badge shows in status bar (existing infrastructure)

#### E3-10: FE — propellant indicators in station panel
- **Crates:** ui_web
- **Changes:** Show LH2 and LOX quantities in station inventory. Show boil-off rate. Visual indicator for propellant reserves.
- **Schema:** None
- **Bench:** None
- **FE:** New elements in station panel

#### E3-11: Run month + quarter scenarios, verify propellant chain
- **Crates:** None (verification)
- **Changes:** Verify full chain: volatile mining → heating → H2O → electrolysis → LH2. Check boil-off pressure. Check power budget with full pipeline running. Verify no collapses.
- **Schema:** None
- **Bench:** Update baseline expectations
- **FE:** None

---

## Epic 4: Propellant-Based Movement

**Goal:** Ships consume LH2 for transit. Propellant tanks. Refuel task. Autopilot planning.

**Depends on:** Epic 2 (hop_dv edges), Epic 3 (LH2 production)
**Blocks:** Nothing (terminal epic)

### Tickets

#### E4-01: Add propellant fields to ShipState
- **Crates:** sim_core (types)
- **Changes:** Add `dry_mass_kg: f32`, `propellant_kg: f32`, `propellant_capacity_kg: f32`, `exhaust_velocity: f32` to `ShipState`. Default values for starter ship: 5000, 50000, 50000, 30000.
- **Schema:** New fields on ShipState
- **Bench:** None
- **FE:** None

#### E4-02: Propellant consumption calculation
- **Crates:** sim_core (tasks or new module)
- **Changes:** Implement `compute_hop_propellant(ship_total_mass, hop_dv, exhaust_velocity) -> f32` using simplified Tsiolkovsky: `total_mass * (1 - 1/exp(dv/ve))`. Implement `compute_route_propellant()` for multi-hop routes.
- **Schema:** None
- **Bench:** None (unit tests)
- **FE:** None

#### E4-03: Deduct propellant on transit start
- **Crates:** sim_core (engine)
- **Changes:** When processing `Command::AssignTask` with Transit, compute total route propellant cost. If insufficient, reject command (emit `InsufficientPropellant` event). Otherwise deduct atomically from `ship.propellant_kg`.
- **Schema:** New event type
- **Bench:** None
- **FE:** None

#### E4-04: Add Refuel task
- **Crates:** sim_core (types, tasks, engine)
- **Changes:** New `TaskKind::Refuel { station_id, target_kg }`. Resolution: each tick, transfer `min(refuel_rate, station_lh2, remaining_need)` from station to ship. Complete when target reached or station empty. Add `refuel_kg_per_tick` constant.
- **Schema:** New TaskKind variant, new constant
- **Bench:** None
- **FE:** None

#### E4-05: Autopilot — propellant check before transit
- **Crates:** sim_control
- **Changes:** Before issuing any Transit command, compute round-trip propellant cost. If ship propellant < cost, issue Refuel first. If station has no LH2, leave ship idle with reason `NoFuel`.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E4-06: Autopilot — auto-refuel on deposit completion
- **Crates:** sim_control
- **Changes:** After a Deposit task completes, if ship propellant < capacity and station has LH2, issue Refuel before next transit.
- **Schema:** None
- **Bench:** None
- **FE:** None

#### E4-07: Update dev_base_state.json with ship propellant
- **Crates:** sim_world, content
- **Changes:** Add propellant fields to ship state in dev_base_state.json. Starting propellant: 50,000 kg (full tank). Update `build_initial_state()`.
- **Schema:** None
- **Bench:** Rerun baselines
- **FE:** None

#### E4-08: Add propellant metrics
- **Crates:** sim_core (metrics)
- **Changes:** Add `fleet_propellant_kg`, `fleet_propellant_pct`, `propellant_consumed_per_tick` to MetricsSnapshot. Track cumulative propellant consumed.
- **Schema:** New metric fields
- **Bench:** New CSV columns
- **FE:** None

#### E4-09: Propellant alert rules
- **Crates:** sim_daemon (alerts)
- **Changes:** "Low ship propellant" when any ship below 20% tank. "Fleet grounded — no fuel" when all ships idle due to NoFuel.
- **Schema:** None
- **Bench:** None
- **FE:** Alert badges (existing)

#### E4-10: FE — propellant gauge on ship cards
- **Crates:** ui_web
- **Changes:** Show propellant_kg / propellant_capacity_kg as a gauge on ship cards in fleet panel. Show estimated range (hops remaining at current mass).
- **Schema:** None
- **Bench:** None
- **FE:** New element in fleet panel

#### E4-11: Add LH2 to pricing.json
- **Crates:** None (content only)
- **Changes:** Add LH2 and LOX as importable/exportable items in pricing.json. LH2 priced high (incentivize local production).
- **Schema:** None
- **Bench:** None
- **FE:** Economy panel shows new items

#### E4-12: Run full scenario suite, verify movement economics
- **Crates:** None (verification)
- **Changes:** Run baseline, month, quarter. Verify ships refuel correctly, propellant consumed matches expectations, fleet doesn't get stranded, inner vs outer belt mining has different propellant costs.
- **Schema:** None
- **Bench:** Update baseline expectations, possibly new scenario
- **FE:** None

---

## Epic 5: Research Expansion

**Goal:** New techs that gate and improve energy, propulsion, and efficiency systems.

**Depends on:** Techs reference systems from Epics 1–4
**Implementation:** Can be added incrementally as each epic lands

### Tickets

#### E5-01: Add tech_solar_efficiency — improved solar array output
- **Crates:** None (content only — techs.json)
- **Changes:** New tech: `tech_solar_efficiency`. Domain: Engineering 150 + Materials 50. Difficulty: 400. Effect: `SolarOutputMultiplier { factor: 1.5 }`. Prereqs: none.
- **Schema:** New TechEffect variant
- **Bench:** None
- **FE:** None

#### E5-02: Implement SolarOutputMultiplier effect
- **Crates:** sim_core (station, research)
- **Changes:** When computing solar array output, check if `tech_solar_efficiency` is unlocked. If so, multiply output by factor. Pattern: `if research.unlocked.contains(&tech_id) { output *= factor }`.
- **Schema:** New TechEffect variant in types
- **Bench:** None
- **FE:** None

#### E5-03: Add tech_electrolysis_efficiency — reduced power cost
- **Crates:** None (content only)
- **Changes:** New tech: `tech_electrolysis_efficiency`. Domain: Materials 200 + Engineering 100. Difficulty: 500. Effect: `ProcessorPowerReduction { module_def: "module_electrolysis_unit", factor: 0.6 }`. Prereqs: none.
- **Schema:** New TechEffect variant
- **Bench:** None
- **FE:** None

#### E5-04: Implement ProcessorPowerReduction effect
- **Crates:** sim_core (station)
- **Changes:** When computing module power consumption, check for ProcessorPowerReduction effects on unlocked techs. If matching module_def, multiply power_consumption_per_run by factor.
- **Schema:** New TechEffect variant in types
- **Bench:** None
- **FE:** None

#### E5-05: Add tech_cryo_insulation — reduced boil-off
- **Crates:** None (content only)
- **Changes:** New tech: `tech_cryo_insulation`. Domain: Materials 250. Difficulty: 600. Effect: `BoiloffReduction { factor: 0.25 }`. Prereqs: none.
- **Schema:** New TechEffect variant
- **Bench:** None
- **FE:** None

#### E5-06: Implement BoiloffReduction effect
- **Crates:** sim_core (engine/station)
- **Changes:** In boil-off tick step, check for BoiloffReduction techs. If unlocked, multiply boiloff_rate by factor (0.25 = 75% reduction).
- **Schema:** New TechEffect variant in types
- **Bench:** Verify in month scenario
- **FE:** None

#### E5-07: Add tech_efficient_propulsion — improved exhaust velocity
- **Crates:** None (content only)
- **Changes:** New tech: `tech_efficient_propulsion`. Domain: Engineering 300 + Materials 100. Difficulty: 700. Effect: `ExhaustVelocityMultiplier { factor: 1.5 }`. Prereqs: `tech_ship_construction`.
- **Schema:** New TechEffect variant
- **Bench:** None
- **FE:** None

#### E5-08: Implement ExhaustVelocityMultiplier effect
- **Crates:** sim_core (tasks or engine)
- **Changes:** When computing propellant consumption, check for ExhaustVelocityMultiplier. Multiply ship's exhaust_velocity by factor. Reduces propellant cost by ~33% at 1.5x.
- **Schema:** New TechEffect variant in types
- **Bench:** Verify propellant savings
- **FE:** None

#### E5-09: Add tech_battery_storage — improved battery capacity
- **Crates:** None (content only)
- **Changes:** New tech: `tech_battery_storage`. Domain: Engineering 200. Difficulty: 450. Effect: `BatteryCapacityMultiplier { factor: 2.0 }`. Prereqs: none.
- **Schema:** New TechEffect variant
- **Bench:** None
- **FE:** None

#### E5-10: Implement BatteryCapacityMultiplier effect
- **Crates:** sim_core (station)
- **Changes:** Battery max_capacity scaled by tech multiplier when computing charge/discharge.
- **Schema:** New TechEffect variant in types
- **Bench:** None
- **FE:** None

#### E5-11: Run full scenario suite with all techs
- **Crates:** None (verification)
- **Changes:** Long-run scenario (quarter+) verifying tech unlock timing for new techs. Verify effects apply correctly. Verify research stagnation doesn't prevent unlocks (labs need appropriate data types).
- **Schema:** None
- **Bench:** New scenario with tech-focused analysis
- **FE:** None

---

## Dependency Graph

```
Epic 1 (Energy) ─────────────┐
                              ├──→ Epic 3 (Water→Propellant) ──→ Epic 4 (Movement)
Epic 2 (Asteroid Typing) ────┘

Epic 5 (Research) ── incremental, follows each epic
```

## Implementation Order

1. **Sprint 1:** Epic 1 (E1-01 through E1-12) + Epic 2 (E2-01 through E2-06) in parallel
2. **Sprint 2:** Epic 2 remainder (E2-07 through E2-10) + Epic 3 (E3-01 through E3-11)
3. **Sprint 3:** Epic 4 (E4-01 through E4-12)
4. **Throughout:** Epic 5 tickets land as their prerequisite systems exist

Each sprint ends with a working build, passing tests, and updated sim_bench baselines.
