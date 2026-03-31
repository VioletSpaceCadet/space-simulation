---
title: "feat: P5 — Station Construction & Multi-Station Expansion"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P5: Station Construction & Multi-Station Expansion

## Overview

The ability to build new stations anywhere in the solar system, creating multi-station supply chains and spatial expansion gameplay. This project absorbs the **Station Frame+Slot System** project (VIO-490 to VIO-496) and adds station construction, deployment via construction ships, inter-station logistics, and FleetCoordinator. The **Strategic Layer + Multi-Station AI** project feeds into P6 (AI Intelligence) as its natural home.

**5 milestones, ~20-25 tickets.** This is the largest P-project because it combines structural diversity (frames), construction mechanics (kits, construction ships), deployment, module delivery, and multi-station coordination.

**Absorbed projects:**
- Station Frame+Slot System (7 existing tickets → moved into P5 Milestone 1)
- Strategic Layer + Multi-Station AI → referenced, absorbed into P6

**Construction ship concept:** Deploying a station in a remote zone (belt, outer system) requires a construction ship — a specialized hull type that carries station kits and performs on-site assembly. This mirrors real-world practices (crane ships, construction vessels) and creates meaningful fleet composition decisions.

## Problem Statement

After P2 (ground ops) and P4 (satellites), the player has a ground facility and orbital infrastructure (satellites, potentially one orbital station via P2's ground-to-orbit transition). But there's no way to expand — no second station, no belt operations, no supply chains. The simulation stops at "one station + one ground facility." Multi-station gameplay is where the sim transforms from a factory game into a logistics and strategy game.

## Proposed Solution

### Phase Structure

| Milestone | Scope | Tickets |
|---|---|---|
| **M1: Station Frames** | FrameDef types, content, slot validation, frame bonuses (absorbs Frame+Slot project) | ~7 |
| **M2: Station Construction** | Station kits, construction ship hull, deployment command, construction task | ~5 |
| **M3: Module Delivery** | Ships ferry modules between stations, station bootstrapping from empty | ~3 |
| **M4: Inter-Station Logistics** | FleetCoordinator, transfer objectives, supply/demand balancing | ~5 |
| **M5: Integration** | Milestones, scenarios, scoring, zone-gated resources | ~4 |

### Station Frames (M1)

Mirrors ship hull+slot architecture. Frames define station archetypes with typed module slots, base stats, and frame bonuses. **This is the existing Frame+Slot project (VIO-490 to VIO-496) absorbed into P5.**

| Frame | Slots | Power | Cargo | Bonus | Role |
|---|---|---|---|---|---|
| Outpost | 4-6 (2 util, 2 industrial, 1 research) | 30 kW | 500 m3 | — | Cheap forward base |
| Industrial Hub | 8-12 (3 util, 4 industrial, 2 research, 1 structural) | 60 kW | 2000 m3 | +10% processing yield | Production center |
| Research Station | 6-8 (2 util, 1 industrial, 4 research, 1 structural) | 40 kW | 800 m3 | +25% research speed | Science platform |

Per `docs/brainstorms/station-frames-requirements.md`: unified slot architecture, frame bonuses via ModifierSet pipeline, advisory slot enforcement (strict enforcement deferred).

### Construction Ship (M2, NEW)

A new hull type for station deployment at remote locations. The construction ship:
- Carries a station kit in cargo
- Transits to target location
- Performs assembly (new TaskKind::ConstructStation, takes N ticks)
- Creates empty StationState with the specified frame at the target position
- Returns to home station after construction

**Why a construction ship, not instant deployment:** Building a station in the belt is a major investment. The construction ship creates meaningful preparation (build the ship, build the kit, plan the route) and risk (ship in transit, fuel for round trip). It also prevents trivially spamming stations — you need fleet capacity allocated to construction.

**Construction ship hull:** `hull_construction_vessel` — large cargo (for kits), low speed (heavy), no mining capability. Slot types: utility + structural. Required tech: `tech_station_construction` (Tier 3).

**Alternative: Launch from ground.** For Earth-orbit stations, P2's launch system can launch station kits directly. Construction ships are for remote deployment. Both paths create the same StationState — the delivery mechanism differs.

### Station Kit Manufacturing (M2)

Station kits are heavy manufactured items produced at ground facilities or orbital stations:

| Kit | Mass (kg) | Recipe Chain | Required Tech |
|---|---|---|---|
| Outpost Kit | 5,000 | structural_beams + solar_cells + circuits + hull_plating | tech_station_construction |
| Industrial Hub Kit | 15,000 | structural_beams (x4) + solar_array + circuits (x2) + processing_unit + hull_plating (x3) | tech_advanced_construction |
| Research Station Kit | 10,000 | structural_beams (x2) + solar_cells + circuits (x3) + optics_array + hull_plating (x2) | tech_advanced_construction |

### Module Delivery (M3)

Once a station is deployed empty, it needs modules. Ships ferry modules from existing stations (or ground facility launches supply runs):

- **Ship cargo:** Modules are inventory items with mass/volume. Ships carry them.
- **New ShipObjective:** `Transfer { from_station, to_station, items }` — ship picks up items at from_station, transits, deposits at to_station.
- **Autopilot module delivery:** When a new station has empty slots and another station has spare modules, autopilot assigns a transfer.
- **Seed supplies:** Station kits include ~500 ticks of basic supplies (propellant, repair kits) so the station doesn't immediately fail.

### Inter-Station Logistics (M4)

The big coordination problem — multiple stations with different resources need to share materials:

- **FleetCoordinator** — new agent layer above StationAgent in `AutopilotController`. Evaluates global supply/demand across all stations. Assigns inter-station transfer objectives to ships.
- **Supply/demand calculation:** Each station reports: what it produces (surplus), what it needs (deficit). FleetCoordinator matches surpluses to deficits.
- **Ship assignment:** Dedicated hauler ships (hull_transport_hauler) assigned to logistics routes. Mining ships stay local.
- **Transfer priority:** Propellant > repair_kits > ore > materials > components (critical supplies first).
- **home_station field:** Ships have `home_station` (from Strategic Layer Phase D, VIO-486). Logistics ships may have no home_station (fleet-level assignment).

### Zone-Gated Resources (M5)

Scan site replenishment gated by zone access:
- P4 comm relays enable zones (CommTier::Basic+)
- New scan sites only appear in zones with comm relay coverage
- Belt zones have richer resources (more Fe, volatiles) but require comm infrastructure + supply chain
- Creates the "invest before exploit" pattern: deploy comm relay → build outpost → mine richer resources

## Technical Approach

### Architecture

```
sim_core/src/types/content.rs (EXTEND)
  ├── FrameDef (from Frame+Slot: slots, bonuses, base stats)
  ├── StationKitDef (kit mass, recipe, frame_id)
  └── ConstructionTaskDef (assembly ticks, requirements)

sim_core/src/types/state.rs (EXTEND)
  ├── StationState.frame_id: Option<FrameId>
  └── StationState uses FacilityCore (from P2 Ticket 0a)

sim_core/src/types/commands.rs (EXTEND)
  ├── Command::DeployStation { ship_id, kit_item, position }
  └── FacilityId already handles Station | Ground (from P2)

sim_core/src/station/ (EXTEND)
  └── recompute_station_stats() — frame bonuses via ModifierSet

sim_control/src/lib.rs (EXTEND)
  ├── FleetCoordinator — new layer above StationAgent
  ├── Supply/demand evaluation
  └── Transfer objective assignment

content/frame_defs.json (NEW, from Frame+Slot VIO-491)
content/hull_defs.json (EXTEND with hull_construction_vessel)
content/recipes.json (EXTEND with station kit + construction ship recipes)
```

### Tick Integration

No new tick steps. Station construction uses existing patterns:
- Construction ship task resolves in `resolve_ship_tasks()` (step 3) — creates StationState on completion
- New station ticks in `tick_stations()` (step 4) — uses FacilityCore from P2
- FleetCoordinator runs in autopilot's `generate_commands()` — before station agents

### FleetCoordinator Design

```
AutopilotController::generate_commands() order:
  1. Lifecycle sync (create/remove agents for new/deleted entities)
  2. FleetCoordinator evaluates global supply/demand (NEW)
  3. FleetCoordinator assigns transfer objectives to logistics ships (NEW)
  4. Station agents generate commands (modules, labs, crew, trade)
  5. Station agents assign ship objectives (mining, survey — local ships only)
  6. Ship agents execute objectives
```

The FleetCoordinator sits between lifecycle sync and station agents. It sees all stations, all ships, and all inventories. It assigns global-level objectives (transfers) before stations assign local objectives (mining). This prevents conflicts — a ship assigned to a transfer won't also be assigned to mine.

## Implementation Tickets

### Milestone 1: Station Frames (absorbs Frame+Slot VIO-490 to VIO-496)

**These are the existing 7 tickets, moved into P5 project:**

#### Ticket 1 (VIO-490): SF-01 — FrameId, FrameDef, ModifierSource::Frame data model

Already detailed in VIO-490. FrameId newtype, FrameDef struct, frame_id on StationState, ModifierSource::Frame variant.

**Size:** Medium | **Existing ticket: VIO-490**

---

#### Ticket 2 (VIO-491): SF-02 — Content: frame_defs.json + module compatible_slots

Already detailed in VIO-491. 3 frame definitions (Outpost, Industrial Hub, Research Station), compatible_slots on all station modules.

**Size:** Medium | **Existing ticket: VIO-491**

---

#### Ticket 3 (VIO-492): SF-03 — Content loading + validation for frames

Already detailed in VIO-492. Load frame_defs.json, validate, update build_initial_state().

**Size:** Small-Medium | **Existing ticket: VIO-492**

---

#### Ticket 4 (VIO-493): SF-04 — recompute_station_stats + frame bonus application

Already detailed in VIO-493. Frame bonuses via modifier pipeline.

**Size:** Small-Medium | **Existing ticket: VIO-493**

---

#### Ticket 5 (VIO-494): SF-05 — InstallModule slot validation

Already detailed in VIO-494. Slot validation on module installation.

**Size:** Medium | **Existing ticket: VIO-494**

---

#### Ticket 6 (VIO-495): SF-06 — Autopilot slot awareness

Already detailed in VIO-495. manage_modules() checks slot availability.

**Size:** Small-Medium | **Existing ticket: VIO-495**

---

#### Ticket 7 (VIO-496): SF-07 — API + FE data layer + event sync

Already detailed in VIO-496. Frame defs in content API, FE types, event sync.

**Size:** Small-Medium | **Existing ticket: VIO-496**

---

### Milestone 2: Station Construction & Deployment

#### Ticket 8: Construction ship hull type + recipe

**What:** New hull type `hull_construction_vessel` for deploying stations at remote locations.

**Details:**
- hull_defs.json: construction_vessel — large cargo (100+ m3 for kits), low speed (slow, heavy), utility + structural slots
- Required tech: tech_station_construction (Tier 3 from P3)
- Manufacturing recipe: complex (structural_beams + propulsion_unit + guidance_system + cargo_module)
- Ship stats computed from hull + modules (existing pattern)
- No mining capability — pure logistics/construction role

**Acceptance criteria:**
- [ ] hull_construction_vessel defined in content
- [ ] Recipe produces construction ship via existing shipyard assembler
- [ ] Ship stats computed correctly from hull
- [ ] Tech gating prevents early construction
- [ ] Unit test: hull loads, recipe produces correct ship type

**Dependencies:** M1 complete (frames exist), P3 tech gating
**Size:** Medium

---

#### Ticket 9: Station kit content — recipes + kit items

**What:** Station kit items and their manufacturing recipes. Kits are heavy inventory items that contain everything needed to deploy a specific frame type.

**Details:**
- 3 kit types: outpost_kit (5,000 kg), industrial_hub_kit (15,000 kg), research_station_kit (10,000 kg)
- Recipes in recipes.json: multi-step chains from manufactured components
- Kit items are InventoryItem::Component with kit-specific component_id
- Each kit specifies which frame_id it deploys
- Tech gating: tech_station_construction for outpost, tech_advanced_construction for industrial/research

**Acceptance criteria:**
- [ ] 3 kit recipes defined in content
- [ ] Assembler produces kit items
- [ ] Kit items reference correct frame types
- [ ] Integration test: manufacture outpost kit from components

**Dependencies:** M1 (frame types exist to reference)
**Size:** Medium

---

#### Ticket 10: Command::DeployStation + TaskKind::ConstructStation

**What:** Ship command to deploy a station using a kit. Construction ship transits to location, performs assembly (N ticks), creates empty StationState.

**Details:**
- Command::DeployStation { ship_id, kit_item_index, target_position }
- Validation: ship has construction capability (hull tag or slot), kit in cargo, valid target position, comm relay in zone (from P4)
- Creates TaskKind::ConstructStation { frame_id, position, assembly_ticks }
- During construction: ship is stationary, "building" for N ticks (configurable per kit, e.g., 48-168 ticks)
- On completion: creates empty StationState at position with frame from kit. Kit consumed. Ship returns to idle.
- Events: StationConstructionStarted, StationDeployed { station_id, frame_id, position }
- **Ground launch alternative:** P2 launch system can launch station kits to earth_orbit. On arrival, instant creation (no construction ship needed for Earth orbit).

**Acceptance criteria:**
- [ ] DeployStation command validates prerequisites
- [ ] Ship transits to location, then constructs for N ticks
- [ ] New StationState created on completion with correct frame
- [ ] Kit consumed from cargo
- [ ] Events emitted (+ FE handlers per event sync)
- [ ] Ground launch path creates station on arrival (integrates with P2 VIO-557)
- [ ] Unit test: full deploy lifecycle
- [ ] Integration test: construction ship builds outpost in belt

**Dependencies:** Ticket 8 (construction ship), Ticket 9 (kits), P4 comm gating (zone must be enabled)
**Size:** Large

---

#### Ticket 11: Station construction events + FE handlers

**What:** New Event variants for station construction lifecycle.

**Details:**
- StationConstructionStarted { ship_id, frame_id, position, assembly_ticks }
- StationDeployed { station_id, frame_id, position }
- StationModuleDelivered { station_id, module_def_id, from_station_id }
- FE handlers in applyEvents.ts
- ci_event_sync.sh updated

**Acceptance criteria:**
- [ ] 3 new Event variants compile and serialize
- [ ] applyEvents.ts handles all 3
- [ ] ci_event_sync.sh passes

**Dependencies:** Ticket 10
**Size:** Small

---

#### Ticket 12: Empty station bootstrapping — seed supplies + first modules

**What:** Newly deployed stations start empty but need minimum supplies to survive. Kit deployment includes seed supplies. First module delivery is critical path.

**Details:**
- Station kit includes seed supplies (defined per kit in content): ~500 ticks of propellant, repair kits, basic Fe
- Empty station has frame but no modules — can't do anything until modules arrive
- Autopilot must prioritize first module delivery to new stations
- Critical supplies (power → then maintenance → then production modules) in priority order
- Station without power modules: all modules stalled. Power first.

**Acceptance criteria:**
- [ ] Kit deployment includes seed supplies from kit definition
- [ ] Empty station survives on seed supplies for 500+ ticks
- [ ] Autopilot prioritizes new station module delivery
- [ ] Integration test: station deployed → modules delivered → station operational

**Dependencies:** Ticket 10, M3 (module delivery)
**Size:** Medium

---

### Milestone 3: Module Delivery & Station Equipment

#### Ticket 13: ShipObjective::Transfer — inter-station item transfer

**What:** Ships can transfer inventory items between stations. New ship objective type for picking up items at one station and delivering to another.

**Details:**
- ShipObjective::Transfer { from_facility: FacilityId, to_facility: FacilityId, items: Vec<ItemSpec> }
- Ship transits to from_facility → picks up items → transits to to_facility → deposits
- Uses existing deposit/pickup mechanics where possible
- Transfer items: modules, materials, components, propellant
- Ship must have sufficient cargo capacity for the transfer payload

**Acceptance criteria:**
- [ ] Transfer objective moves items between two stations
- [ ] Ship handles transit → pickup → transit → deposit sequence
- [ ] Cargo capacity respected
- [ ] Unit test: transfer objective lifecycle
- [ ] Integration test: module transferred from station A to station B

**Dependencies:** Ticket 10 (stations exist to transfer between)
**Size:** Medium-Large

---

#### Ticket 14: Autopilot module delivery to new stations

**What:** Autopilot identifies new/empty stations and schedules module deliveries from stations with spare inventory.

**Details:**
- Detect stations with empty slots matching available modules elsewhere
- Priority: power modules first → maintenance → production → research
- Schedule Transfer objectives for module delivery
- Source station: the station with the module in inventory (or manufacture it if possible)
- Respect frame slot types — only deliver modules that fit available slots

**Acceptance criteria:**
- [ ] Autopilot detects empty station needing modules
- [ ] Schedules transfers for priority modules
- [ ] Respects slot type constraints
- [ ] Integration test: new station receives modules and becomes operational

**Dependencies:** Ticket 13, M1 (slot awareness)
**Size:** Medium

---

#### Ticket 15: Station specialization via zone resources

**What:** Different zones have different resource profiles, naturally creating station specialization. Belt zones have richer Fe and volatiles. Earth orbit has trade access. Outer zones have exotic materials.

**Details:**
- Zone resource profiles defined in solar_system.json (already has resource_class per zone)
- Scan site templates weighted by zone resource class
- Station frame bonuses align with zone resources (mining outpost in belt, research station anywhere)
- Autopilot evaluates zone resources when deciding where to build next station
- Content-driven: new zone resources addable via JSON

**Acceptance criteria:**
- [ ] Zone resource profiles influence scan site composition
- [ ] Station specialization emerges from zone + frame combination
- [ ] Autopilot considers zone resources in expansion decisions
- [ ] Integration test: belt outpost produces more ore than earth orbit station

**Dependencies:** M1 (frames), Ticket 10 (deployment)
**Size:** Medium

---

### Milestone 4: Inter-Station Logistics

#### Ticket 16: FleetCoordinator — global supply/demand evaluation

**What:** New agent layer in AutopilotController that evaluates supply/demand across all stations and identifies transfer needs.

**Details:**
- `FleetCoordinator` struct in sim_control, managed by AutopilotController
- Runs before station agents in generate_commands()
- Evaluates per-station: surplus (producing more than consuming), deficit (consuming more than producing)
- Identifies transfer opportunities: station A has surplus Fe, station B needs Fe
- Priority ranking: propellant > repair_kits > ore > materials > components
- Produces `Vec<TransferPlan>` consumed by ship assignment logic

**Acceptance criteria:**
- [ ] FleetCoordinator identifies surpluses and deficits across stations
- [ ] Transfer plans created for supply/demand mismatches
- [ ] Priority ranking respected
- [ ] Unit test: surplus at A, deficit at B → transfer plan generated
- [ ] Deterministic: same state → same transfer plans

**Dependencies:** M3 (transfer objectives exist)
**Size:** Large

---

#### Ticket 17: Logistics ship assignment — dedicated haulers

**What:** FleetCoordinator assigns transfer objectives to available ships. Dedicated hauler ships prioritized for logistics. Mining ships stay on local tasks.

**Details:**
- Ship role classification: hull_transport_hauler → logistics, hull_mining_barge → mining, hull_construction_vessel → construction
- FleetCoordinator assigns TransferPlan to available logistics ships
- Ships with home_station (from Strategic Layer VIO-486 → P6) prefer transfers involving their home station
- Ships without home_station are fleet-level logistics — assigned wherever needed
- Prevent conflicts: ship assigned to transfer is not available for mining by station agents

**Acceptance criteria:**
- [ ] Hauler ships assigned to logistics routes
- [ ] Mining ships not pulled into logistics
- [ ] Transfer objectives execute correctly (pickup → transit → deposit)
- [ ] No double-assignment conflicts with station agents
- [ ] Integration test: 2 stations, 1 hauler, materials flow from A to B

**Dependencies:** Ticket 16
**Size:** Medium-Large

---

#### Ticket 18: Cross-station asteroid claim map

**What:** When multiple stations exist, prevent two stations from assigning ships to the same asteroid. Distance-based claiming.

**Details:**
- BTreeMap<AsteroidId, StationId> built as pre-pass in AutopilotController
- Each asteroid claimed by nearest station (distance-based, deterministic tiebreak by StationId)
- Station agents only assign ships to their claimed asteroids
- Replaces current all-asteroids-available model
- (This is Phase D4 from Strategic Layer: VIO-488 — absorbed here since it only matters with multiple stations)

**Acceptance criteria:**
- [ ] Asteroids claimed by nearest station
- [ ] No double-assignment across stations
- [ ] Deterministic tiebreak
- [ ] Unit test: 2 stations, 5 asteroids → correct partition
- [ ] Integration test: multi-station mining without conflicts

**Dependencies:** Ticket 16 (FleetCoordinator framework)
**Size:** Medium

---

#### Ticket 19: Ship home_station field + construction assignment

**What:** Ships have a home_station field. Assigned on construction. Logistics ships may be fleet-level (no home_station).

**Details:**
- home_station: Option<StationId> on ShipState with serde(default)
- Ships built at a station get home_station = that station
- Ships with None assigned to nearest station on first controller tick
- Station-scoped ship assignment: station agents only consider their home ships for mining
- (This is Phase D2 from Strategic Layer: VIO-486)

**Acceptance criteria:**
- [ ] home_station field on ShipState, backward compatible
- [ ] Ships assigned home_station on construction
- [ ] Station agents only assign home ships
- [ ] Fleet-level ships (None) available for FleetCoordinator logistics

**Dependencies:** None (can be early, improves multi-station correctness)
**Size:** Small-Medium

---

#### Ticket 20: Supply chain metrics + advisor integration

**What:** Track inter-station transfer volume, route utilization, and supply chain health. Include in AdvisorDigest and MetricsSnapshot.

**Details:**
- New metrics: transfer_volume_kg (total moved between stations per tick window), route_count (active logistics routes), supply_chain_health (0-1, ratio of fulfilled to requested transfers)
- AdvisorDigest extension: supply chain summary
- Bottleneck detection: identify stations with unfulfilled deficits
- MCP advisor: query_knowledge can filter by supply chain status

**Acceptance criteria:**
- [ ] Transfer metrics tracked in MetricsSnapshot
- [ ] AdvisorDigest includes supply chain summary
- [ ] Bottleneck detection identifies unfulfilled stations

**Dependencies:** Ticket 17 (logistics running)
**Size:** Small-Medium

---

### Milestone 5: Integration & Validation

#### Ticket 21: Station construction milestones

**What:** Progression milestones for station construction, extending P1 milestone system.

**Details:**
- first_station_constructed — deploy any station → $50M grant
- belt_outpost — station deployed in belt zone → $40M grant + unlock belt mining
- multi_station_logistics — 2+ stations with active transfer route → $30M grant
- research_station — research station deployed with 3+ labs → $25M grant
- self_sustaining_network — 3+ stations, positive net balance → $100M grant

**Acceptance criteria:**
- [ ] 5 station milestones defined
- [ ] Milestones trigger correctly
- [ ] Integration test: autopilot reaches first_station_constructed

**Dependencies:** P1 milestone system (VIO-533), Ticket 10
**Size:** Small-Medium

---

#### Ticket 22: sim_bench multi-station scenarios

**What:** Scenarios validating station construction and multi-station operations.

**Details:**
- station_construction.json: ground_start, 20000 ticks, 10 seeds. First station deployed, modules delivered, operational.
- multi_station_logistics.json: 2-station state, 10000 ticks, 20 seeds. Inter-station transfers, supply chain health.
- belt_expansion.json: ground_start, 40000 ticks, 5 seeds. Full arc: ground → orbit → belt outpost.
- CI smoke includes quick station check.

**Acceptance criteria:**
- [ ] Construction scenario validates end-to-end deployment
- [ ] Logistics scenario validates inter-station transfers
- [ ] Belt expansion validates full progression arc
- [ ] CI smoke includes station check

**Dependencies:** M4 complete
**Size:** Medium

---

#### Ticket 23: Score dimension extension for multi-station

**What:** P0 scoring dimensions account for station construction and logistics.

**Details:**
- Expansion: station count, zone coverage with stations
- Industrial: multi-station combined output
- Efficiency: supply chain utilization (transfers fulfilled / requested)
- Fleet: construction missions + logistics routes

**Acceptance criteria:**
- [ ] Multi-station activity contributes to scoring
- [ ] Score improves when second station operational

**Dependencies:** P0 scoring (VIO-521)
**Size:** Small

---

#### Ticket 24: Multi-station regression test (100 seeds)

**What:** Automated regression test ensuring multi-station construction and logistics work across seeds.

**Details:**
- Quick test (5 seeds, 20000 ticks): first station constructed, modules delivered
- Full test (100 seeds, 40000 ticks, ignored): belt outpost operational, logistics flowing
- Both use real content
- Validates: no asteroid double-assignment, no logistics deadlock, stations receive modules

**Acceptance criteria:**
- [ ] Quick test passes in cargo test
- [ ] Full test passes with --ignored
- [ ] 90%+ of seeds achieve first station construction
- [ ] Zero logistics deadlocks

**Dependencies:** M4 complete
**Size:** Medium

---

## Dependency Graph

```
Milestone 1: Station Frames (VIO-490 to VIO-496)
  Tickets 1-7 (existing, sequential per Frame+Slot project)
       │
Milestone 2: Construction
  Ticket 8 (construction ship) ──→ Ticket 10 (deploy command)
  Ticket 9 (station kits)     ──→ Ticket 10
                                       │
                                  Ticket 11 (events)
                                  Ticket 12 (bootstrapping)
       │
Milestone 3: Module Delivery
  Ticket 13 (transfer objective) ──→ Ticket 14 (autopilot delivery)
  Ticket 15 (zone specialization)
       │
Milestone 4: Logistics
  Ticket 19 (home_station — can start early) ←── independent
  Ticket 16 (FleetCoordinator) ──→ Ticket 17 (logistics assignment)
  Ticket 18 (asteroid claim map)           │
                                     Ticket 20 (metrics)
       │
Milestone 5: Integration
  Tickets 21-24 (milestones, scenarios, scoring, regression)
```

**Critical path:** M1 → Ticket 10 (deploy command) → Ticket 13 (transfers) → Ticket 16 (FleetCoordinator) → Ticket 22 (scenarios)

## Absorbed Projects

| Original Project | Disposition |
|---|---|
| **Station Frame+Slot System** (VIO-490 to VIO-496) | Absorbed into P5 Milestone 1. Move tickets to P5 project. Mark standalone project as completed/absorbed. |
| **Strategic Layer + Multi-Station AI** (VIO-479 to VIO-489) | Phase C (StrategyConfig, VIO-479-484) → P6 (AI Intelligence). Phase D (multi-station, VIO-485-489): VIO-486 (home_station) and VIO-488 (claim map) absorbed into P5 M4. VIO-485, VIO-487, VIO-489 → P6. |

## P6 Interface

P5 delivers the infrastructure that P6 (AI Intelligence) optimizes:

| P5 Delivers | P6 Uses It For |
|---|---|
| FleetCoordinator | P6 adds StrategyConfig to control FleetCoordinator priorities |
| Ship home_station | P6 adds station-scoped ship assignment optimization |
| Multi-station state | P6 adds cross-station strategy evaluation |
| Transfer objectives | P6 optimizes transfer routing via AutopilotConfig |
| Supply chain metrics | P6 uses for strategy evaluation (which stations need help) |

## Risk Analysis

### High Risk

**Inter-station logistics deadlock** — station B needs materials from station A, but no ship available to transfer.
- *Mitigation:* Station kits include seed supplies. Autopilot prioritizes building transport hauler before deploying stations. FleetCoordinator detects unfulfilled transfers and escalates (build more haulers, or import via trade).

### Medium Risk

**Construction ship is expensive** — player may not be able to afford one early in the game.
- *Mitigation:* Earth orbit stations can be launched via P2 ground launch (no construction ship needed). Construction ships needed only for belt+ stations. Station construction grant ($50M) funds the effort.

**FleetCoordinator complexity** — global optimization across multiple stations is algorithmically harder than per-station decisions.
- *Mitigation:* Start simple: greedy matching (closest surplus to closest deficit). Optimize in P6. The Design Spine says "introduce one pressure system, observe, tune."

## Sources & References

### Origin
- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 5 section
- **Station frames:** [docs/brainstorms/station-frames-requirements.md](../../docs/brainstorms/station-frames-requirements.md)
- **Entity depth:** [docs/brainstorms/entity-depth-requirements.md](../../docs/brainstorms/entity-depth-requirements.md) (R7-R12)

### Absorbed Linear Projects
- Station Frame+Slot System: VIO-490 to VIO-496 (7 tickets → P5 M1)
- Strategic Layer + Multi-Station AI: VIO-479-484 → P6, VIO-486+488 → P5 M4, VIO-485+487+489 → P6

### Related Linear
- VIO-507: Plan P5 (planning ticket — mark done)
- P2: Ground ops launch system (VIO-557 — station kit launches)
- P3: Tech gating (tech_station_construction, tech_advanced_construction)
- P4: Comm relay zone gating (VIO-569 — required for remote station deployment)
