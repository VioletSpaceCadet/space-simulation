---
title: "feat: P2 — Ground Operations, Sensors & Launch System"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

## Enhancement Summary

**Deepened on:** 2026-03-30
**Research agents used:** Launch economics (real SpaceX data), Space observation instruments (5 types), Codebase learnings (10 applicable), Architecture review, Pattern analysis

### Critical Architecture Findings

1. **Extract shared `FacilityCore` before Ticket 2** — Both `StationState` and `GroundFacilityState` need modules, inventory, crew, power. Without a shared abstraction, every module behavior change must be applied in two parallel dispatch loops. Add a "Ticket 0" to extract module-hosting helpers. Per CLAUDE.md: "Make the change easy, then make the change."

2. **Migrate `DataKind` to content-driven strings** — CLAUDE.md says `DataKind` and `ResearchDomain` are content-driven strings. They are currently Rust enums (4 variants each). The plan proposes adding 4 more enum variants, contradicting the convention. **Resolution:** Add a prerequisite ticket to migrate `DataKind` to `String` (like `AnomalyTag`), then sensor data kinds are pure content additions. This prevents code changes for every new sensor type.

3. **Introduce `FacilityId` enum for command polymorphism** — Existing commands use `station_id: StationId`. Ground facilities need the same commands (Import, Export, InstallModule, SetModuleEnabled). Rather than duplicating every command variant, introduce `FacilityId::Station(StationId) | Ground(GroundFacilityId)` and migrate commands to use it.

4. **Put `operating_cost_per_tick` on `ModuleDef`, not a wrapper type** — The plan proposes `FacilityModuleState` wrapping `ModuleState`. This breaks existing patterns. Instead, add `operating_cost_per_tick: f64` (default 0.0) to `ModuleDef`. Orbital modules have cost 0. Ground modules have cost > 0. No wrapper needed.

5. **Trim LaunchPayload to YAGNI scope** — Ship with `Supplies(Vec<InventoryItem>)` and `StationKit` only. `Satellite(SatelliteDefId)` references P4 types that don't exist. `Crew` has no consumer yet. Add variants when those systems land.

6. **Start with 2-3 sensor types, not 5** — Design Spine says "never stack 3 new entropy sources at once." Start with Optical + Radio (cheapest, most fundamental). Add IR/Spectroscopy/Radar as content-only expansions after the system is validated. The architecture supports this — sensor types are content-driven strings.

### Key Learnings Applied (from docs/solutions/)

| Learning | Application to P2 |
|---|---|
| **module-behavior-extensibility** (19-step checklist) | Follow for EACH new module type — LaunchPad needs all 8 match arm locations + ModuleKindState variant |
| **cross-layer-feature-development** | Phase types first (Phase 0), then core behavior, then integration. GroundFacilityState types with `serde(default)` before writing any behavior. |
| **crew-system-multi-ticket** | Model per-module operating costs as a gate on `should_run()`, identical to crew satisfaction pattern. Disabled modules = zero cost. |
| **hierarchical-agent-decomposition** | Design `GroundFacilityAgent` with lifecycle sync in `AutopilotController`. Follow BTreeMap pattern. |
| **gameplay-deadlock** | Trace every dependency chain from ground milestones back to starting state. Validate with 100-seed sim_bench runs. |
| **event-sync-enforcement** | Run `ci_event_sync.sh` after each ticket adding Event variants. Plan FE handlers upfront. |

### Real-World Calibration Data

**Launch costs (2024-2026):** Sounding $1-5M (50-500kg suborbital), Small $7-20M (300-1750kg), Medium $67M reusable / $90M expendable (15-23K kg, Falcon 9), Heavy $97-150M (64K kg, Falcon Heavy). SpaceX marginal cost for reused Falcon 9: ~$18-28M (charges $67M = 60-70% gross margin).

**Reusability curve:** 45% cost reduction on 2nd flight, 80% by 5th, 85-90% by 10th+. Max reuse: 23 flights on single booster (B1058). Turnaround: 21 days (best) to 60 days (typical). Propellant is ~0.3% of launch cost — the vehicle is everything.

**Sensor progression (real-world order):** Optical survey (cheapest, $3-6M/yr, discovers objects) → Spectrograph follow-up ($1-5M/yr, classifies composition) → IR characterization ($5-15M/yr, determines true size) → Radar when close-approach available ($5-10M/yr, 3D shape + density) → Radio for subsurface properties ($15-30M/yr). Key: radar only works on close-approach objects (within ~0.1-0.2 AU) — natural scarcity mechanic.

**SpaceX timeline:** Founding to first orbit: 6.3 years. First orbit to first landing: 7.2 years. First landing to routine reuse: ~4.3 years. Total founding to routine reuse: ~18 years.

---

# P2: Ground Operations, Sensors & Launch System

## Overview

The "chapter one" of the game — an Earth-based operations center where the player builds a space company from the ground up. Buy sensors and manufacturing equipment, research the sky with diverse instrument types, manufacture rocket components, and develop a launch capability that evolves from expensive expendable rockets to cost-effective reusable vehicles. This is the SpaceX origin story: garage to orbit.

**Key departures from original P2 plan:**
- **GroundFacility is a new entity type**, not a tagged StationState. Earth bases have fundamentally different economics (bought equipment, per-module operating costs, near-instant trade).
- **Sensors are diverse and content-driven** — optical, radio, radar, infrared, spectroscopy — not a single "telescope" module.
- **Launch system has moderate depth** — rocket types, reusability progression, launch pads. Not a simple mass × cost abstraction.
- **Same manufacturing/trade engine** — assemblers and refineries work the same, but Earth trade is near-instant (~1 week delivery, not a year-long wait).
- **Timeline is flexible** — progression could take game-months or game-years. Design the systems, tune via content constants and sim_bench.
- **P1 stays as-is** — P2 adds an Earth-based starting state as a precursor path alongside the existing orbital start.

**The progression arc this creates:**

```
Earth Surface (GroundFacility)
  │
  ├── Buy basic sensors → observe space
  ├── Buy basic manufacturing → build components
  ├── Research with diverse sensors → unlock tech
  ├── Manufacture rocket components + buy bought parts → assemble rockets
  ├── Build launch pad → launch payloads
  ├── First satellite launch (small expendable rocket)
  ├── First supply launch to orbit
  ├── Develop reusable rockets → reduce launch cost
  ├── Launch station kit to orbit → P1 orbital station created
  │
  └── Transition to orbital operations (existing P1 gameplay)
```

## Problem Statement

The current simulation starts in orbit with a fully-equipped station. There's no "earning space" — no ground-based R&D, no launch development, no progression from Earth to orbit. The most compelling arc in real space industry — building capability from nothing on Earth to a functioning orbital presence — is entirely missing.

**Real-world parallel:** SpaceX went from a garage (2002) to Falcon 1 orbit (2008) to reusable Falcon 9 (2015) to Starship (2020s). That's ~6-15 years of ground-based development before a mature launch capability. The simulation should capture this arc — compressed but recognizable.

## Proposed Solution

### GroundFacility Entity

A new entity type representing an Earth-based (or planetary surface) operations center. Fundamentally different from orbital StationState:

| Aspect | GroundFacility (Earth) | StationState (Orbit) |
|---|---|---|
| **Trade** | Near-instant (~168 ticks / 1 week at mpt=60) | Delayed (P1 milestone-gated, then available) |
| **Module acquisition** | Buy modules via trade | Build/import modules (slower, more expensive) |
| **Operating costs** | Per-module OpEx (monthly cost per active module) | Crew salary only (modules have no running cost) |
| **Mining** | Not available (no asteroids on Earth) | Ships mine asteroids |
| **Manufacturing** | Assembler with bought inputs | Assembler with mined/refined inputs |
| **Sensors** | 5 types (optical, radio, radar, IR, spectroscopy) | Sensor array (orbital) |
| **Launch capability** | Yes (rockets from launch pads) | No (direct deployment) |
| **Ships** | Not dockable (no mining ships) | Ships dock for mining/deposit |
| **Position** | Surface (parent_body: earth, radius: 0) | Orbit (parent_body: zone, radius > 0) |

**Why new entity type (not tagged StationState):** The economic model is fundamentally different. StationState assumes modules are free to operate once installed, trade is delayed and expensive, and raw materials come from mining. GroundFacility assumes modules cost money to run, trade is fast and cheap, and you buy inputs rather than mine them. Stretching StationState to handle both creates a confused abstraction. (User decision: new entity type.)

### Per-Module Operating Costs

Each active module on a GroundFacility has a monthly operating cost (power, maintenance, consumables, staff support). This creates meaningful decisions about what to keep running:

| Module Type | Example Monthly Cost | Rationale |
|---|---|---|
| Optical Telescope | $50,000 | Power, cooling, maintenance |
| Radio Telescope Array | $150,000 | Power-hungry, large infrastructure |
| Radar System | $200,000 | High power consumption |
| IR Sensor | $75,000 | Cryogenic cooling |
| Spectrograph | $100,000 | Precision optics maintenance |
| Basic Assembler | $80,000 | Operating staff, materials handling |
| Advanced Assembler | $200,000 | Clean room, precision tools |
| Launch Pad (Small) | $100,000 | Ground support equipment |
| Launch Pad (Large) | $500,000 | Major infrastructure |
| Research Lab | $120,000 | Staff, equipment, supplies |

**Implementation:** `operating_cost_per_tick: f64` field on `ModuleDef` (default 0.0 via `serde(default)`). Orbital station modules have cost 0. Ground modules have cost > 0. Deducted from balance each tick for enabled modules at ground facilities. This is separate from crew salary (which already exists). Follows the crew satisfaction gate pattern from `docs/solutions/patterns/crew-system-multi-ticket-implementation.md` — disabled modules incur zero cost, computed per-tick before module dispatch.

### Sensor Diversity

Five content-driven sensor types, each producing different data kinds useful for different research domains:

| Sensor Type | What It Observes | Data Kind | Research Domain | Discovery Capability |
|---|---|---|---|---|
| **Optical Telescope** | Visible light, NEO detection, asteroid brightness | OpticalData | Survey | Discover near-Earth scan sites |
| **Radio Telescope** | Deep space signals, pulsar timing, composition hints | RadioData | Materials | Detect distant asteroid fields |
| **Radar System** | Orbital debris, close-approach objects, surface mapping | RadarData | Engineering | Precise orbital elements, collision warnings |
| **Infrared Sensor** | Thermal signatures, asteroid size/classification | InfraredData | Survey | Classify asteroid composition (Fe-rich vs volatile-rich) |
| **Spectrograph** | Detailed composition analysis, molecular signatures | SpectroscopyData | Materials | Detailed composition without physical samples |

**Key design:** Sensor types are content-driven strings, not enum variants (per codebase convention — content-driven types are strings, engine mechanics are enums). `SensorArrayDef` gets a `sensor_type: String` field. Data kinds may remain an enum since they're engine mechanics (data flows into research), but adding new variants is straightforward.

**Progression through sensors:**
1. Start with bought basic optical telescope (cheap, discovers nearby objects)
2. Unlock radio telescope via tech (deeper space observation)
3. Manufacture specialized IR sensor (requires components from assembler + bought optics)
4. Advanced spectrograph (requires tech unlock from IR sensor data + manufactured precision components)
5. Radar system (requires tech unlock, provides operational intelligence for launches)

### Launch System

Moderate-depth rocket system with types, reusability progression, and launch pads:

#### Rocket Types (content-driven via `content/rockets.json`)

| Rocket | Payload to LEO | Base Launch Cost | Reusable? | Required Tech |
|---|---|---|---|---|
| Sounding Rocket | 50 kg | $500K | No | None (starter) |
| Light Launcher | 500 kg | $5M | No | tech_basic_rocketry |
| Medium Launcher | 5,000 kg | $30M | Expendable initially | tech_medium_rocketry |
| Heavy Launcher | 20,000 kg | $100M | Expendable initially | tech_heavy_rocketry |
| Reusable Medium | 4,000 kg (reduced) | $10M (after dev) | Yes | tech_reusable_rockets |
| Reusable Heavy | 15,000 kg (reduced) | $35M (after dev) | Yes | tech_reusable_heavy |

**Reusability progression (SpaceX parallel):**
1. **Expendable** — full cost per launch, rocket destroyed
2. **Partial recovery** — booster recovery, 60% cost reduction, requires tech + launch pad upgrade
3. **Full reuse** — both stages recovered, 70% cost reduction, requires advanced tech
4. Each reuse tier requires R&D (tech unlocks) + manufacturing capability (build reusable components)

#### Launch Pads (module type for GroundFacility)

Launch pads are modules installed at the ground facility:
- **Small Pad** — supports sounding rockets + light launchers. Cheap to buy and operate.
- **Medium Pad** — supports up to medium launchers. Requires infrastructure tech.
- **Large Pad** — supports heavy launchers. Major investment.
- **Reusable Pad** — landing zone for booster recovery. Required for reusable rockets.

#### Launch Command Flow

```
1. Player (autopilot) issues Command::Launch { facility_id, rocket_type, payload, destination }
2. System checks: rocket available (manufactured), pad available (correct size), payload fits, fuel available
3. Launch cost deducted from balance
4. Payload transits from ground to destination (earth_orbit_zone, specific orbital position)
   - Transit time based on rocket type (sounding: 1 tick, heavy: 3-5 ticks)
5. At destination: payload deployed
   - Satellite → creates SatelliteState (P4)
   - Station kit → creates empty StationState (P5)
   - Supplies → adds to station inventory
   - Crew → adds to station crew
6. If reusable: rocket returns to facility (recovery time), available for next launch
7. If expendable: rocket consumed
8. Events emitted: PayloadLaunched, LaunchCompleted, (BoosterRecovered if reusable)
```

### Earth Manufacturing

Same assembler/refinery system as orbital stations, but with Earth-specific recipes and bought inputs:

**Example Earth manufacturing chains:**
```
Bought: aluminum sheet + bought: electronics + manufactured: guidance system
  → Assembled: satellite bus

Bought: fuel grain + bought: oxidizer tank + manufactured: nozzle assembly
  → Assembled: solid rocket motor

Manufactured: turbo pump + manufactured: combustion chamber + bought: propellant lines
  → Assembled: liquid engine

Assembled: solid rocket motor × 2 + assembled: satellite bus + bought: fairing
  → Assembled: light launcher (rocket)
```

**Key insight:** Early rockets use mostly bought components (expensive but fast). As tech progresses, more components are manufactured in-house (cheaper per unit but requires manufacturing line investment). This mirrors real industry: SpaceX initially bought engines, then developed Merlin in-house.

## Technical Approach

### Architecture

```
sim_core/src/types/facility_core.rs (NEW — shared module-hosting abstraction)
  ├── FacilityCore { modules, inventory, crew, cargo_capacity, power, module_type_index, ... }
  └── Shared by both StationState and GroundFacilityState

sim_core/src/types/ground_facility.rs (NEW)
  ├── GroundFacilityId (newtype)
  ├── GroundFacilityState { id, position, core: FacilityCore, launch_transits, ... }
  └── LaunchPadState { pad_type, available, recovery_countdown, launches_count }
  └── LaunchTransitState { rocket_def_id, payload, destination, arrival_tick }

sim_core/src/types/rocket.rs (NEW)
  ├── RocketDef { id, name, payload_capacity_kg, base_cost, fuel_kg, reusable, required_tech, pad_size }
  ├── RocketState (inventory item with reuse_count metadata, not a separate entity)
  └── ReusabilityTier enum { Expendable, PartialRecovery, FullReuse }

sim_core/src/types/commands.rs (MODIFIED)
  ├── FacilityId enum { Station(StationId), Ground(GroundFacilityId) }
  └── Migrate Import/Export/InstallModule/SetModuleEnabled to use FacilityId

content/ground_facilities.json (NEW) — facility definitions
content/rockets.json (NEW) — rocket type definitions
content/module_defs.json (EXTENDED) — sensor variants + launch pad + operating_cost_per_tick field

GameState additions:
  pub ground_facilities: BTreeMap<GroundFacilityId, GroundFacilityState>
```

### Architecture Decision Records

**ADR-1: FacilityCore shared abstraction (from architecture review)**

Both StationState and GroundFacilityState need: modules, inventory, crew, power, module_type_index, cargo_capacity. Without a shared type, every module behavior change must be applied in two parallel dispatch loops (tick_stations AND tick_ground_facilities). This is the "parallel class hierarchy" anti-pattern.

**Resolution:** Extract `FacilityCore` struct containing shared fields. Subsystem tickers (sensor, lab, assembler, maintenance) operate on `&mut FacilityCore` instead of hardcoded `StationId` lookups. This is a prerequisite before Ticket 2 (new "Ticket 0"). Per CLAUDE.md: "Make the change easy, then make the change."

**ADR-2: DataKind AND ResearchDomain as content-driven strings (from pattern review + code audit)**

CLAUDE.md says both `DataKind` and `ResearchDomain` should be content-driven strings. Both are currently Rust enums (4 variants each) with a hardcoded 1:1 mapping in `domain_to_data_kind()` at `sim_events.rs:597`. The engine doesn't branch on specific values — it passes them through to the research system.

**Resolution:** VIO-544 migrates BOTH DataKind AND ResearchDomain to String newtypes (like AnomalyTag). The `domain_to_data_kind()` mapping becomes content-defined. Then sensor data kinds AND new research domains are pure content additions.

**ADR-3: FacilityId command polymorphism (from architecture review)**

Existing commands use `station_id: StationId`. Ground facilities need the same commands. Options: (a) duplicate every command variant, (b) introduce FacilityId enum.

**Resolution:** Introduce `FacilityId::Station(StationId) | Ground(GroundFacilityId)`. Migrate Import, Export, InstallModule, SetModuleEnabled to use FacilityId. Command::Launch uses facility_id directly.

**ADR-4: Rockets as inventory items (from architecture review)**

Reusable rockets need identity (reuse_count, condition). Options: (a) separate tracked entity, (b) inventory item with metadata.

**Resolution:** Rockets are inventory items. On expendable launch, consumed. On reusable launch, recovery creates a new item with updated reuse_count. Uses existing inventory mechanics. Quality field or new ItemKind variant carries reuse metadata. Avoids a separate entity tracking system.

**ADR-5: LaunchTransitState on facility (from architecture review)**

In-transit payloads need state. Options: (a) list on facility, (b) ephemeral ship, (c) instant with delay.

**Resolution:** `Vec<LaunchTransitState>` on GroundFacilityState with `{ rocket_def_id, payload, destination, arrival_tick }`. Resolved at tick step 3.5. Clean, no ship system pollution.

### Tick Integration

Ground facility ticking slots into the existing cycle:

```
1. Apply commands (including Command::Launch)
2. Deduct crew salaries (existing — extend to ground facility crew)
2.5 Deduct module operating costs (NEW — ground facility modules only)
3. Resolve ship tasks (existing)
3.5 Resolve launch tasks (NEW — payload transit, booster recovery)
4. Tick stations (existing)
4.5 Tick ground facilities (NEW — sensors, labs, assemblers, launch pads)
5. Advance research (existing — now includes ground sensor data)
5.5 Evaluate milestones (P1)
6. Replenish scan sites
7. Increment tick
```

### Earth Trade Integration

Ground facilities use the existing trade system with modified parameters:
- **Trade delay:** Near-instant for Earth facilities. Add `trade_delay_ticks: u64` field to facility def (default: 168 = 1 week at mpt=60). Import commands queued and fulfilled after delay.
- **Pricing:** Same `pricing.json` — Earth facilities pay the same prices but without orbital delivery surcharge. Could add `earth_import_surcharge_per_kg` (lower than `import_surcharge_per_kg`).
- **No trade tier gating:** Earth facilities always have full trade access (they're on Earth, buying from Earth suppliers).

## Implementation Tickets

### Milestone 0: Architecture Prerequisites

#### Ticket 0a: Extract FacilityCore shared abstraction from StationState

**What:** Extract shared module-hosting fields from `StationState` into a `FacilityCore` struct. Refactor subsystem tickers to operate on `&mut FacilityCore`.

**Why first:** Without this, Ticket 2 (ground facility tick integration) would duplicate the entire module dispatch pipeline. "Make the change easy, then make the change."

**Details:**
- Extract from StationState: modules, inventory, crew, cargo_capacity, power, module_type_index, module_id_index, power_budget_cache, cached_inventory_volume_m3
- StationState becomes: `{ id, position, core: FacilityCore, thermal_links, modifiers, ... }`
- Refactor `tick_stations()` subsystem calls (sensor, lab, assembler, maintenance, processor) to take `&mut FacilityCore` instead of looking up `state.stations[station_id]`
- All existing behavior preserved — this is a pure refactoring

**Acceptance criteria:**
- [ ] FacilityCore struct extracts shared fields
- [ ] StationState composes FacilityCore
- [ ] All subsystem tickers parameterized over FacilityCore
- [ ] All existing tests pass with zero behavior change
- [ ] cargo clippy clean

**Dependencies:** None
**Size:** Large (significant refactoring, many files)

---

#### Ticket 0b: Migrate DataKind from enum to content-driven strings

**What:** Convert `DataKind` from a 4-variant Rust enum to a content-driven `String` (like `AnomalyTag`). This allows adding sensor data kinds via content JSON without code changes.

**Why:** CLAUDE.md convention: "Content-driven types: AnomalyTag, DataKind, ResearchDomain are loaded from content JSON." DataKind is currently an enum, violating this. P2 needs 4+ new data kinds for sensors — doing this as enum variants means code changes per sensor type.

**Details:**
- Change `DataKind` from enum to `pub struct DataKind(pub String)` (newtype wrapper)
- Update all match arms on DataKind to use string comparison or content lookup
- Existing values become strings: "SurveyData", "AssayData", "ManufacturingData", "TransitData"
- Update content files (techs.json, module_defs.json) to use string data kinds
- SensorArrayDef.data_kind already uses DataKind — transparent change

**Acceptance criteria:**
- [ ] DataKind is a String newtype
- [ ] All existing behavior preserved
- [ ] New data kinds addable via content JSON only (no Rust changes)
- [ ] All tests pass
- [ ] CLAUDE.md convention now matches reality

**Dependencies:** None
**Size:** Medium

---

#### Ticket 0c: FacilityId command polymorphism

**What:** Introduce `FacilityId` enum that can reference either a StationId or GroundFacilityId. Migrate shared commands (Import, Export, InstallModule, SetModuleEnabled) to use FacilityId.

**Details:**
- `FacilityId::Station(StationId) | Ground(GroundFacilityId)` enum
- Commands that operate on any facility use FacilityId: Import, Export, InstallModule, UninstallModule, SetModuleEnabled, SetModuleThreshold
- Commands specific to stations keep StationId: AssignShipTask, etc.
- Commands specific to ground facilities use GroundFacilityId: Launch
- `apply_commands()` dispatches based on FacilityId variant
- StationContext and equivalent ground context resolve from the appropriate entity

**Acceptance criteria:**
- [ ] FacilityId enum exists
- [ ] 6+ command variants migrated to FacilityId
- [ ] apply_commands correctly dispatches to station or ground facility
- [ ] All existing tests pass (StationId → FacilityId::Station migration)
- [ ] Serialization backward compatible

**Dependencies:** Ticket 0a (FacilityCore exists), Ticket 1 (GroundFacilityId type exists)
**Size:** Medium-Large

---

### Milestone 1: Ground Facility Foundation

#### Ticket 1: GroundFacility entity type + state + content schema

**What:** Define `GroundFacilityState`, `GroundFacilityId`, and content schema. Add to `GameState`. Load from content.

**Details:**
- `GroundFacilityState`: id, name, position (earth surface), modules (Vec<ModuleState>), inventory, crew, cargo_capacity, power state
- `GroundFacilityId` newtype (like StationId)
- Add `ground_facilities: BTreeMap<GroundFacilityId, GroundFacilityState>` to GameState
- Content: `content/ground_facilities.json` with facility definitions
- Serialization: round-trips through JSON, backward compatible (empty map default)
- Position: `parent_body: "earth", radius_au_um: 0, angle_mdeg: 0`

**Acceptance criteria:**
- [ ] GroundFacilityState compiles, serializes, deserializes
- [ ] GameState loads with empty ground_facilities by default (backward compatible)
- [ ] Content schema documented in docs/reference.md
- [ ] Unit test: facility state round-trips through JSON

**Dependencies:** None
**Size:** Medium

---

#### Ticket 2: Ground facility tick integration

**What:** Integrate ground facilities into the tick cycle. Tick modules (sensors, labs, assemblers) on ground facilities using existing module ticking logic.

**Details:**
- New function `tick_ground_facilities()` called from `tick()` at step 4.5
- Reuse existing module ticking code where possible (sensors, labs, assemblers all work the same mechanically)
- Ground facilities need their own `module_type_index` for routing
- Power computation for ground facilities (can use same PowerState logic)
- Ground facilities don't have ships docking — no deposit/mining tasks

**Acceptance criteria:**
- [ ] Ground facility modules tick each cycle
- [ ] Sensors generate data, labs process data, assemblers produce components
- [ ] Power system works for ground facilities
- [ ] Integration test: ground facility with sensor produces research data over 100 ticks
- [ ] TickTimings extended to include ground facility step

**Dependencies:** Ticket 1
**Size:** Large (significant integration work)

---

#### Ticket 3: Per-module operating costs

**What:** Active modules on ground facilities incur per-tick operating costs deducted from balance.

**Details:**
- Add `operating_cost_per_tick: f64` to `ModuleBehaviorDef` or as a separate content field per module (0.0 for orbital station modules, >0 for ground modules)
- New tick step 2.5: iterate ground facility modules, sum operating costs for enabled modules, deduct from balance
- Disabled modules don't incur cost (incentivizes shutting down unused equipment)
- Emit event when operating costs push balance negative (warning)
- Operating costs are content-defined per module (tunable)

**Acceptance criteria:**
- [ ] Active modules deduct operating cost each tick
- [ ] Disabled modules have zero operating cost
- [ ] Cost values content-driven (not hardcoded)
- [ ] Unit test: enabling/disabling modules changes cost
- [ ] Integration test: balance decreases correctly over 100 ticks with 3 active modules
- [ ] Existing orbital station modules unaffected (cost = 0)

**Dependencies:** Ticket 2
**Size:** Small-Medium

---

#### Ticket 4: Earth trade — near-instant import/export for ground facilities

**What:** Ground facilities can import/export with near-instant delivery (configurable delay, ~1 week default). No milestone-based trade gating for Earth facilities.

**Details:**
- Add `trade_delay_ticks: u64` field to facility config (default: 168 ticks = 1 week at mpt=60)
- Import commands on ground facilities bypass milestone-based trade gating (always available)
- Import fulfillment: either instant or queued with delay (pending_imports list, fulfilled after delay)
- Pricing: same pricing.json but could have reduced `import_surcharge_per_kg` for Earth
- Export from ground facilities: available immediately (selling manufactured goods)

**Acceptance criteria:**
- [ ] Ground facility imports work without trade tier unlock
- [ ] Import delay configurable via content (default ~1 week)
- [ ] Pricing applies correctly to ground facility trade
- [ ] Orbital station trade unchanged (still milestone-gated per P1)
- [ ] Unit test: import on ground facility succeeds at tick 0
- [ ] Unit test: import on orbital station respects trade tier

**Dependencies:** Ticket 2, P1 trade gating (VIO-535)
**Size:** Medium

---

#### Ticket 5: Earth-based starting state (ground_start.json)

**What:** New starting state that begins with an Earth ground facility instead of an orbital station. Alternative progression path: Earth → orbit.

**Details:**
- `content/ground_start.json` — starts with:
  - 1 GroundFacility on Earth surface
  - Basic optical telescope (bought, in inventory)
  - Basic assembler (bought, in inventory)
  - 1 research lab
  - Small solar array (or grid power — Earth has different power model)
  - 4 crew: 2 operators, 1 technician, 1 scientist
  - $50M starting balance
  - No ships, no orbital station, no scan sites (must discover via sensors)
- sim_cli supports `--state ground_start.json`
- sim_bench scenarios can specify this starting state
- **Does NOT replace progression_start.json** — this is an alternative path

**Acceptance criteria:**
- [ ] ground_start.json loads without error
- [ ] sim_cli runs from ground_start for 1000+ ticks
- [ ] Ground facility sensors produce data
- [ ] Assembler can process bought components
- [ ] Balance doesn't crash before first revenue opportunity

**Dependencies:** Tickets 1-4
**Size:** Medium

---

### Milestone 2: Sensor Suite

#### Ticket 6: Sensor type diversity — content schema + SensorArrayDef extension

**What:** Extend SensorArrayDef with `sensor_type` field. Define initial 2-3 sensor types in content (optical + radio + optionally spectroscopy). Architecture supports adding IR/radar/spectroscopy later via content-only changes.

**Research insight (real-world progression):** Optical surveys are cheapest ($3-6M/yr) and discover objects. Radio provides deep-space detection and subsurface data ($15-30M/yr). Spectroscopy classifies composition ($1-5M/yr). IR determines true size ($5-15M/yr). Radar is highest-value but works only on close-approach objects within ~0.1-0.2 AU — a natural scarcity mechanic.

**Start small, expand via content:** Design Spine says "never stack 3 new entropy sources at once." Ship with 2-3 sensor types. The content-driven architecture (Ticket 0b) means adding more types is a JSON change, not a code change.

**Details:**
- Add `sensor_type: String` field to `SensorArrayDef`
- Initial types: `optical`, `radio` (minimum viable), optionally `spectroscopy`
- Each type maps to a specific `data_kind` and `research_domain`
- Different scan intervals, power requirements, and operating costs
- Backward compatible: existing sensor_array modules get `sensor_type: "orbital"` or similar default

**Sensor content definitions:**
```json
{
  "module_optical_telescope": {
    "sensor_type": "optical",
    "data_kind": "SurveyData",
    "scan_interval_minutes": 120,
    "operating_cost_per_tick": 0.83
  },
  "module_radio_telescope": {
    "sensor_type": "radio",
    "data_kind": "RadioData",
    "scan_interval_minutes": 240,
    "operating_cost_per_tick": 2.5
  }
}
```

**Acceptance criteria:**
- [ ] SensorArrayDef has sensor_type field
- [ ] 5 sensor module definitions in content
- [ ] Each type produces correct data kind
- [ ] Existing orbital sensor_array unaffected
- [ ] Unit test: each sensor type generates expected data kind

**Dependencies:** Ticket 2 (ground facility ticking)
**Size:** Medium

---

#### Ticket 7: Sensor-specific data kinds + research domain mapping

**What:** Add new DataKind variants for sensor-specific data. Map to research domains via tech system.

**Details:**
- New DataKind variants: `RadioData`, `RadarData`, `InfraredData`, `SpectroscopyData` (SurveyData already exists for optical)
- Each data kind feeds specific research domains:
  - OpticalData (SurveyData) → Survey domain
  - RadioData → Materials domain (composition hints)
  - RadarData → Engineering domain (orbital mechanics)
  - InfraredData → Survey domain (classification)
  - SpectroscopyData → Materials domain (detailed composition)
- Tech definitions reference these data kinds in their requirements
- Research advancement uses all data from all sensor types

**Acceptance criteria:**
- [ ] 4 new DataKind variants compile
- [ ] Each maps to correct research domain
- [ ] Techs can require specific data kinds
- [ ] Existing research system handles new data kinds
- [ ] Unit test: radio telescope data feeds materials domain research

**Dependencies:** Ticket 6
**Size:** Medium

---

#### Ticket 8: Sensor discovery capabilities

**What:** Different sensors can discover different things — optical finds nearby objects, radio detects distant fields, IR classifies composition, radar maps orbits.

**Details:**
- Optical: discovers scan sites in near-Earth zones (like current sensor_array)
- Radio: discovers scan sites in distant zones (belt, outer system) at lower rate
- Radar: provides orbital element data (future: launch window optimization)
- IR: classifies discovered asteroids (adds composition tags without deep scan)
- Spectroscopy: detailed composition data (like deep scan but remote, lower accuracy)
- Discovery events emitted with sensor type attribution

**Acceptance criteria:**
- [ ] Optical discovers near-Earth scan sites
- [ ] Radio discovers distant scan sites (belt+)
- [ ] IR provides composition classification for discovered asteroids
- [ ] Spectroscopy provides remote composition data
- [ ] Each sensor type has unique discovery behavior
- [ ] Integration test: ground facility with 3 sensor types discovers + classifies over 500 ticks

**Dependencies:** Ticket 7
**Size:** Medium-Large

---

#### Ticket 9: Autopilot sensor management for ground facilities

**What:** Ground facility autopilot agent that manages sensor operations — deciding which sensors to buy, enable, and how to prioritize observation targets.

**Details:**
- New `GroundFacilityAgent` (or extend StationAgent with facility support)
- Decides: which sensors to buy (via import), which to enable (balance operating costs vs data value)
- Prioritizes sensor purchases based on progression needs (optical first for discovery, then radio for deep space)
- Manages lab assignments based on available sensor data
- Budget-aware: disables expensive sensors if balance is low

**Acceptance criteria:**
- [ ] Autopilot buys sensors via trade when budget allows
- [ ] Enables/disables sensors based on budget
- [ ] Lab assignments align with available sensor data
- [ ] Integration test: autopilot-managed ground facility discovers asteroids and produces research within 500 ticks

**Dependencies:** Ticket 8, Ticket 4 (Earth trade for buying sensors)
**Size:** Medium

---

### Milestone 3: Launch System Core

#### Ticket 10: Rocket type definitions + RocketDef content schema

**What:** Define rocket types in content. `RocketDef` struct with payload capacity, cost, fuel requirements, required tech, reusability tier.

**Details:**
- `content/rockets.json` with 4-6 rocket types (sounding through heavy)
- `RocketDef`: id, name, payload_capacity_kg, base_launch_cost, fuel_kg, required_tech, reusability_tier, pad_size_required
- `ReusabilityTier` enum: Expendable, PartialRecovery, FullReuse
- Load in `sim_world::load_content()` as part of GameContent
- Validation: required_tech references exist, pad sizes valid

**Acceptance criteria:**
- [ ] content/rockets.json with 4+ rocket types
- [ ] RocketDef compiles and deserializes
- [ ] Loaded as part of GameContent
- [ ] Schema documented in docs/reference.md
- [ ] Unit test: rocket defs load and validate

**Dependencies:** None (parallel with foundation work)
**Size:** Small-Medium

---

#### Ticket 11: Launch pad module type

**What:** New module type for ground facilities — launch pads of different sizes supporting different rocket classes.

**Details:**
- New ModuleBehaviorDef variant: `LaunchPad(LaunchPadDef)` with pad_size (Small/Medium/Large), supports_recovery (bool)
- `LaunchPadState`: available (bool), recovery_countdown (Option<u64>), launches_count
- Launch pads are installed at ground facilities like any module
- Only one launch per pad at a time (pad busy during launch + recovery)
- Recovery pad variant for reusable rockets (booster lands here)
- Operating cost: higher for larger pads

**Acceptance criteria:**
- [ ] LaunchPad module type compiles
- [ ] Can be installed at ground facility
- [ ] Pad availability tracking works (busy during launch)
- [ ] Recovery countdown for reusable launches
- [ ] Unit test: pad transitions between available/busy/recovering states

**Dependencies:** Ticket 2 (ground facility module support), Ticket 10 (rocket types for pad sizing)
**Size:** Medium

---

#### Ticket 12: Launch command + payload delivery

**What:** `Command::Launch` that executes a rocket launch from a ground facility, delivering payload to an orbital destination.

**Details:**
- `Command::Launch { facility_id, rocket_def_id, payload: LaunchPayload, destination: Position }`
- `LaunchPayload` enum: Supplies(Vec<InventoryItem>), StationKit (assembled inventory item, not a new type). Satellite and Crew variants deferred to P4/P5 (YAGNI — those systems don't exist yet).
- Validation: rocket manufactured/available, pad available (correct size), payload within capacity, fuel available
- Cost deducted from balance
- Payload transit: creates temporary transit state (ground → orbit). Duration based on rocket type (1-5 ticks).
- At destination: payload materialized (supplies added to station inventory, satellite created, station created)
- Events: `PayloadLaunched { facility_id, rocket_id, payload_description, cost }`, `LaunchCompleted { payload, destination }`

**Acceptance criteria:**
- [ ] Command::Launch validates all prerequisites
- [ ] Cost deducted on launch
- [ ] Payload arrives at destination after transit delay
- [ ] Supplies, satellites, station kits all deliverable
- [ ] Unit test: launch with valid rocket + pad + payload succeeds
- [ ] Unit test: launch without pad fails gracefully
- [ ] Unit test: launch with overweight payload fails
- [ ] Integration test: end-to-end launch from ground facility to orbit

**Dependencies:** Ticket 10 (rocket types), Ticket 11 (launch pads)
**Size:** Large

---

#### Ticket 13: Launch cost model + fuel consumption

**What:** Economic model for launches — base cost modified by rocket type, payload mass, fuel consumption.

**Details:**
- Base cost from RocketDef
- Fuel cost: `fuel_kg × fuel_cost_per_kg` (content constant)
- Payload surcharge: optional mass-based additional cost for heavy payloads
- Fuel consumed from ground facility inventory (must have propellant manufactured or bought)
- Total launch cost = base_cost + fuel_cost + payload_surcharge
- Cost breakdown included in LaunchCompleted event for analysis
- Reusable rockets: cost formula adjusted by ReusabilityTier (see Ticket 15)

**Acceptance criteria:**
- [ ] Launch cost computed correctly from all components
- [ ] Fuel consumed from facility inventory
- [ ] Launch fails if insufficient fuel
- [ ] Cost breakdown in events for analysis
- [ ] Unit test: cost formula matches expected values for each rocket type

**Dependencies:** Ticket 12
**Size:** Small-Medium

---

#### Ticket 14: Rocket manufacturing — assembler recipes for launch vehicles

**What:** Assembler recipes that produce rockets and rocket components at the ground facility.

**Details:**
- Recipe chain for each rocket type:
  - **Sounding rocket:** bought: solid_fuel_grain + bought: guidance_unit + manufactured: nozzle → sounding_rocket
  - **Light launcher:** manufactured: solid_motor × 2 + manufactured: guidance_system + bought: fairing → light_launcher
  - **Medium launcher:** manufactured: liquid_engine + manufactured: fuel_tank + manufactured: guidance_system + bought: fairing → medium_launcher
  - **Heavy launcher:** manufactured: liquid_engine × 3 + manufactured: large_fuel_tank + manufactured: avionics → heavy_launcher
- Sub-component recipes (manufactured from bought raw materials):
  - bought: steel_plate + bought: ceramic_liner → nozzle
  - bought: turbopump_kit + manufactured: combustion_chamber → liquid_engine
  - etc.
- Recipes gated by tech requirements (basic_rocketry, medium_rocketry, etc.)
- Content: extend `content/recipes.json` with rocket recipes

**Acceptance criteria:**
- [ ] Rocket recipes defined in content
- [ ] Full chain from bought inputs to assembled rocket
- [ ] Tech gating on advanced recipes
- [ ] Assembler at ground facility can produce rockets
- [ ] Integration test: ground facility manufactures sounding rocket from bought parts within 200 ticks

**Dependencies:** Ticket 2 (ground facility assembler ticking), Ticket 10 (rocket types to manufacture)
**Size:** Medium-Large (many recipe definitions)

---

### Milestone 4: Launch Development

#### Ticket 15: Reusability system — expendable to full recovery progression

**What:** Rockets can be reusable, reducing per-launch cost. Reusability tiers unlocked via tech.

**Details:**
- **Expendable:** Rocket consumed on launch. Full cost every time.
- **Partial Recovery:** Booster recovers to landing pad. 60% cost reduction on subsequent launches. Rocket has wear (limited reuses). Requires `tech_partial_recovery` + landing pad module.
- **Full Reuse:** Both stages recovered. 70% cost reduction. More reuses before wear-out. Requires `tech_full_reuse`.
- Reusable rockets tracked as inventory items with `reuse_count` and `condition` (wear-like degradation)
- Recovery takes time (recovery_countdown on launch pad, ~24-48 ticks)
- Rocket inspection/refurbishment between flights (maintenance bay can do this, or auto-refurb at cost)
- SpaceX parallel: Falcon 9 went from expendable (2010) to landing (2015) to routine reuse (2017)

**Acceptance criteria:**
- [ ] Expendable launches consume rocket
- [ ] Reusable launches return rocket to facility
- [ ] Cost reduction applies correctly per tier
- [ ] Recovery countdown prevents immediate relaunch
- [ ] Rocket wear increases with reuse count
- [ ] Worn-out rocket must be refurbished or scrapped
- [ ] Unit test: cost difference between expendable and reusable launches
- [ ] Integration test: reusable rocket launches 3 times with decreasing marginal cost

**Dependencies:** Ticket 12 (launch command), Ticket 13 (cost model)
**Size:** Large

---

#### Ticket 16: Launch vehicle R&D — tech tree extension for rocketry

**What:** Extend the tech tree with rocketry-specific technologies that unlock rocket types and reusability.

**Details:**
- New techs in `content/techs.json`:
  - `tech_basic_rocketry` — unlocks sounding rocket + light launcher
  - `tech_medium_rocketry` — unlocks medium launcher (prereq: basic_rocketry)
  - `tech_heavy_rocketry` — unlocks heavy launcher (prereq: medium_rocketry)
  - `tech_partial_recovery` — unlocks partial booster recovery (prereq: medium_rocketry + engineering research)
  - `tech_full_reuse` — unlocks full reusability (prereq: partial_recovery + advanced engineering)
  - `tech_advanced_propulsion` — unlocks efficient engines, reduced fuel consumption
- Each tech requires research domain points from ground sensors (Engineering + Materials domains)
- This creates the research → capability → launch → more research feedback loop

**Acceptance criteria:**
- [ ] 6+ rocketry techs defined in content
- [ ] Tech prerequisites form a sensible tree
- [ ] Techs gate correct rocket types and features
- [ ] Ground sensors produce required domain data
- [ ] Integration test: research at ground facility unlocks basic_rocketry within expected timeframe

**Dependencies:** Ticket 7 (sensor data kinds feed research), Ticket 10 (rocket types gated by tech)
**Size:** Medium

---

### Milestone 5: Integration & Scenarios

#### Ticket 17: Ground facility autopilot — launch decision making

**What:** Autopilot for ground facilities decides when to build and launch rockets. Manages the build → launch → develop cycle.

**Details:**
- Extends GroundFacilityAgent (from Ticket 9) with launch planning:
  - Determines when to manufacture rockets (sufficient components + budget)
  - Chooses launch targets (first: satellites for more data, then: station supplies, eventually: station kit)
  - Manages launch cadence (don't burn budget on too many launches)
  - Prioritizes reusable development when tech is available
- Budget allocation: split between sensor ops, manufacturing, and launches
- Milestone awareness: knows what launches are needed for progression

**Acceptance criteria:**
- [ ] Autopilot manufactures rockets when components + budget allow
- [ ] Autopilot launches when rocket + pad + payload ready
- [ ] Autopilot doesn't bankrupt the company with excessive launches
- [ ] Integration test: autopilot ground facility achieves first satellite launch within expected timeframe

**Dependencies:** Ticket 9 (sensor management), Ticket 14 (rocket manufacturing), Ticket 12 (launch command)
**Size:** Large

---

#### Ticket 18: Ground-to-orbit transition — first orbital station via launch

**What:** The culmination of the ground phase: launching a station kit to orbit, creating the first orbital station. Bridges P2 ground gameplay to P1 orbital gameplay.

**Details:**
- Station kit recipe: manufactured components assembled into a deployable station kit
- Station kit is a heavy payload (5,000-20,000 kg depending on frame)
- Launching a station kit creates an empty StationState at the destination
- The orbital station starts with no modules — must launch additional payloads (modules, supplies, crew)
- This is multiple launches, not a single "build station" action
- The transition from ground-only to ground+orbit is gradual (first supplies, then modules, then crew)

**Acceptance criteria:**
- [ ] Station kit assemblable at ground facility
- [ ] Station kit launchable to orbit (medium/heavy rocket required)
- [ ] Launched kit creates empty StationState at destination
- [ ] Follow-up launches deliver modules, supplies, crew to the new station
- [ ] Integration test: full ground-to-orbit transition over 2000 ticks

**Dependencies:** Ticket 12 (launch command), Ticket 14 (manufacturing)
**Size:** Medium-Large

---

#### Ticket 19: Ground operations milestones

**What:** P1-compatible milestones specific to the ground operations phase. Tracking progress from first sensor observation through first orbital station.

**Details:**
- New milestones in `content/milestones.json`:
  - `first_observation` — ground sensor produces first data → $2M grant
  - `first_asteroid_classified` — IR/spectroscopy classifies an asteroid → $5M grant
  - `first_manufactured_component` — assembler produces first component → $8M grant
  - `first_rocket_assembled` — complete rocket manufactured → $10M grant
  - `first_launch` — successful launch to orbit → $25M grant + unlock orbital operations
  - `first_reusable_landing` — booster recovery successful → $20M grant
  - `first_orbital_station` — station kit deployed in orbit → $50M grant
  - `launch_cost_milestone` — achieve <$5M per launch → $30M grant
- These extend (not replace) P1 milestones — additional progression markers for the ground path

**Acceptance criteria:**
- [ ] 8 ground-specific milestones defined
- [ ] Milestones trigger correctly during ground facility gameplay
- [ ] Grants fund continued operations
- [ ] Integration test: autopilot reaches first_launch milestone from ground_start.json

**Dependencies:** P1 milestone system (VIO-533, VIO-534), Ticket 12 (launches)
**Size:** Medium

---

#### Ticket 20: sim_bench ground operations scenarios

**What:** sim_bench scenarios validating the ground operations phase works end-to-end.

**Details:**
- `scenarios/ground_bootstrap.json` — ground_start.json, 2000 ticks, 20 seeds. Validates: sensors produce data, first component manufactured, basic research progresses.
- `scenarios/ground_launch.json` — ground_start.json, 10000 ticks, 20 seeds. Validates: first rocket manufactured, first launch achieved, first orbital payload delivered.
- `scenarios/ground_to_orbit.json` — ground_start.json, 30000 ticks, 10 seeds. Validates: full transition to orbital station within timeline. Score comparison with orbital-start path.
- `scenarios/ground_economy.json` — ground_start.json, 5000 ticks, 20 seeds. Validates: operating costs manageable, balance positive trajectory after grants, no bankruptcy.

**Acceptance criteria:**
- [ ] ground_bootstrap passes (research + manufacturing within 2000 ticks)
- [ ] ground_launch passes (first launch within 10000 ticks)
- [ ] ground_to_orbit validates full transition
- [ ] ground_economy validates sustainable finances
- [ ] CI smoke includes quick ground operations check

**Dependencies:** Ticket 5 (ground_start.json), Ticket 17 (autopilot), Ticket 19 (milestones)
**Size:** Medium

---

#### Ticket 21: Score dimension extension — ground ops contribute to scoring

**What:** Extend P0 scoring dimensions to account for ground facility activity. Ensure ground-start runs produce meaningful scores.

**Details:**
- **Industrial Output** — include ground facility manufacturing output
- **Research Progress** — include ground sensor data contribution
- **Economic Health** — factor in operating costs, grant income from ground milestones
- **Expansion** — ground facility counts as a base, launches count as expansion activity
- **Efficiency** — operating cost efficiency (data produced per $ spent on sensors)
- Ensure ground-start score trajectories are comparable to orbital-start trajectories

**Acceptance criteria:**
- [ ] Ground facility activity contributes to all relevant scoring dimensions
- [ ] ground_start score trajectory shows meaningful progression
- [ ] Score comparison: ground_start vs progression_start (different curves, both viable)

**Dependencies:** P0 scoring (VIO-521), Ticket 20 (scenario data)
**Size:** Small-Medium

---

## Dependency Graph

```
Milestone 0: Prerequisites
  Ticket 0a (FacilityCore extraction) ──→ Ticket 0c (FacilityId commands)
  Ticket 0b (DataKind to strings)                      │
       │                                                │
Milestone 1: Foundation                                 │
  Ticket 1 (entity type) ──→ Ticket 0c ←───────────────┘
       │                        │
       └── Ticket 2 (tick integration, uses FacilityCore) ──→ Ticket 3 (operating costs)
                                     │                              │
                                     ├──→ Ticket 4 (Earth trade)    │
                                     │                              │
                                     └──→ Ticket 5 (starting state) ←┘

Milestone 2: Sensors (needs 0b for content-driven data kinds)
  Ticket 6 (sensor schema) ──→ Ticket 7 (data kinds, now content-only) ──→ Ticket 8 (discovery)
                                                                                 │
                                                                           Ticket 9 (autopilot)

Milestone 3: Launch Core
  Ticket 10 (rocket defs) ──→ Ticket 11 (launch pads) ──→ Ticket 12 (launch command)
                                                                │
                              Ticket 14 (manufacturing) ←───────┤
                                                                │
                                                          Ticket 13 (cost model)

Milestone 4: Launch Development
  Ticket 15 (reusability) ←── Ticket 12 + 13
  Ticket 16 (tech tree) ←── Ticket 7 + 10

Milestone 5: Integration
  Ticket 17 (launch autopilot) ←── Ticket 9 + 12 + 14
  Ticket 18 (ground-to-orbit) ←── Ticket 12 + 14
  Ticket 19 (milestones) ←── P1 milestone system + Ticket 12
  Ticket 20 (scenarios) ←── Ticket 5 + 17 + 19
  Ticket 21 (scoring) ←── P0 scoring + Ticket 20
```

**Critical path:** 0a → 1 → 0c → 2 → 6 → 7 → 8 (prerequisites → foundation → sensors) and 10 → 11 → 12 → 14 → 17 (rockets → launch → autopilot)

**Three parallel streams after prerequisites:**
- **Stream A (Foundation + Sensors):** 0a → 1 → 0c → 2 → 3/4/5, then 6 → 7 → 8 → 9
- **Stream B (DataKind migration):** 0b (independent, can parallel with 0a)
- **Stream C (Launch):** 10 → 11 → 12 → 13 → 14, then 15 → 16
- **Convergence:** Tickets 17-21 need both Stream A and Stream C

## Risk Analysis

### High Risk

**Ground phase is boring** — if the player can only watch sensors scan and assemblers slowly build, the phase drags.
- *Likelihood:* Medium-High
- *Impact:* High (if the first hours are boring, nobody continues)
- *Mitigation:* Multiple parallel activities (sensor diversity creates variety, manufacturing chains create decisions, budget management creates tension). Fast-forward speed for early game. Grant milestones provide dopamine hits. Validate engagement via sim_bench pacing scenarios.

**GroundFacility integration complexity** — new entity type touches many systems (tick cycle, serialization, commands, autopilot, events, scoring).
- *Likelihood:* High
- *Impact:* Medium (can be built incrementally)
- *Mitigation:* Milestone-based delivery. Milestone 1 (foundation) is self-contained and testable before adding sensors or launches.

### Medium Risk

**Launch system balance** — rocket costs, reusability economics, and payload capacity need careful tuning.
- *Likelihood:* Medium
- *Impact:* Medium (tunable via content)
- *Mitigation:* All values content-driven. sim_bench scenarios test economic viability. Grid search optimization (P0) can tune launch parameters.

**Operating cost model creates bankruptcy** — per-module costs could drain balance if poorly calibrated.
- *Likelihood:* Medium
- *Impact:* High (simulation collapses)
- *Mitigation:* Operating costs content-tunable. Milestones provide grants to offset costs. sim_bench validates positive balance trajectory. Autopilot disables expensive modules when budget is tight.

### Low Risk

**Sensor type proliferation** — 5 sensor types might be too many for early game.
- *Likelihood:* Low
- *Impact:* Low (content-driven, can start with 2-3)
- *Mitigation:* Start with optical + radio for launch, add IR/spectroscopy/radar as tech unlocks. Content-driven means no code change to add/remove sensor types.

## P1 Impact Notes

P2 adds an Earth-based starting state **alongside** P1's orbital start. The two paths:

| Path | Starting State | First 2000 Ticks | Reaches Orbit |
|---|---|---|---|
| **Orbital (P1)** | progression_start.json — orbital station | Mining, refining, milestones | Already in orbit |
| **Full (P2)** | ground_start.json — Earth facility | Sensors, manufacturing, launches | After first station kit launch |

P1 milestones still work for the orbital path. P2 adds ground-specific milestones for the Earth path. Both paths converge at orbital operations. The scoring system (P0) measures both.

**P1 tickets that may need updates:**
- VIO-536 (progression_start.json) — may want to note it's the "orbital start" path
- VIO-533 (milestone evaluation) — must handle ground facility state in conditions
- VIO-538 (autopilot progression) — needs GroundFacilityAgent for Earth path

These can be addressed as follow-up tickets when P2 implementation begins.

## Sources & References

### Origin

- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 2 section. **Significant departures:** new entity type (not tagged StationState), diverse sensors (not single telescope), moderate launch system (not simple cost abstraction), per-module operating costs.

### Internal References

- `crates/sim_core/src/types/state.rs:475-511` — StationState (reference for GroundFacilityState design)
- `crates/sim_core/src/engine.rs:25-95` — tick cycle (integration points for ground facility)
- `crates/sim_core/src/station/sensor.rs` — SensorArray ticking (extend for sensor types)
- `crates/sim_core/src/types/content.rs:635-768` — ModuleBehaviorDef variants (add LaunchPad)
- `crates/sim_core/src/commands.rs:338-476` — trade system (adapt for Earth trade)
- `content/pricing.json` — pricing model (adapt for ground facility economics)
- `content/solar_system.json` — Earth position for ground facility placement

### Related Linear Projects

- P0: Scoring (VIO-519 through VIO-529) — scoring dimensions extended by Ticket 21
- P1: Progression (VIO-530 through VIO-541) — milestone system extended by Ticket 19
- VIO-504: Plan P2 (original planning ticket — will be updated/replaced)
