---
title: "feat: P4 — Satellite & Unmanned Operations"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P4: Satellite & Unmanned Operations

## Overview

Satellites are the first things launched into orbit — cheap, unmanned, persistent orbital assets that prove space capability and provide real value. They bridge P2's ground-based observation with eventual crewed orbital operations, and they are the natural first payload for the P2 launch system.

**Satellite types are content-driven** — defined by behavior strings in `content/satellite_defs.json`, not Rust enum variants. Ship with 4 types (Survey, Communication, Navigation, Science Platform), expandable via content.

**Communication relays use tiered gating** — basic operations possible without relay (manual/slow), relay unlocks automated operations + trade, advanced relay unlocks full speed.

**Why before P3 (Deep Tech Tree):** Satellites don't need a deep tech tree. They need basic rocketry tech from P2 and the launch system. The first satellite launch is likely the second or third milestone in the ground operations phase — the first payload you build a rocket for.

## Problem Statement

After P2, the player has ground-based sensors and launch capability. But what do they launch? Without satellites, the only orbital target is a full station kit (expensive, heavy, late-game). Satellites fill this gap:

- **Low cost** — small, light, launchable on a sounding or light rocket
- **Immediate value** — orbital survey satellite produces better scan data than any ground telescope
- **Infrastructure building** — comm relays open new zones for future operations
- **Ongoing demand** — satellite wear creates continuous manufacturing and launch demand
- **Progression proof** — first successful satellite deployment is a major milestone

## Proposed Solution

### SatelliteState Entity

New lightweight entity in GameState. Simpler than StationState (no modules, no inventory, no crew). Each satellite has a single type-specific behavior.

```
SatelliteState {
    id: SatelliteId,
    def_id: String,              // references satellite_defs.json
    name: String,
    position: Position,          // orbital position (parent_body + radius + angle)
    deployed_tick: u64,
    wear: f64,                   // 0.0 (new) to 1.0 (failed), same as module WearState
    enabled: bool,
    satellite_type: String,      // content-driven: "survey", "communication", "navigation", "science"
    payload_config: Option<String>, // for science sats: which sensor type ("optical", "radio", "ir")
}
```

### Satellite Types (content-driven)

Types defined in `content/satellite_defs.json` as behavior strings, not Rust enums. Each has a mechanical effect evaluated during the satellite tick step.

| Type | Mechanical Effect | Payload Weight | Cost Class | Required Tech |
|---|---|---|---|---|
| **Survey** | Passive scan site discovery in deployed zone. Better than ground optical (orbital vantage). Discovers scan sites at 2x ground rate. | 200-500 kg | Cheap | tech_basic_rocketry |
| **Communication** | Tiered zone enablement. Basic: enables manual operations. Standard: enables autopilot + trade. Advanced: full speed. | 300-800 kg | Moderate | tech_basic_rocketry |
| **Navigation** | Reduces travel time for ships in deployed zone (-10-20% transit ticks). Stacks with multiple nav sats (diminishing returns). | 200-400 kg | Cheap | tech_navigation_systems |
| **Science Platform** | Carries a sensor payload (optical/radio/IR/spectroscopy). Produces data at 3-5x ground sensor rate (no atmosphere, continuous operation). | 500-2000 kg | Expensive | tech_orbital_science |

**Content-driven extensibility:** Adding a new satellite type = adding a JSON entry in `satellite_defs.json` + implementing its behavior function. The satellite tick system dispatches on `satellite_type` string, not a match on an enum.

### Science Platform Satellites

The most interesting type — extends P2's sensor system into orbit:

- Carries one sensor payload specified at manufacturing time (e.g., "orbital_optical_telescope")
- Produces the same data kinds as ground sensors but at 3-5x rate (no atmosphere, no weather, 24/7 operation)
- Higher operating cost (power, attitude control) reflected in higher manufacturing cost
- Creates a meaningful choice: invest in a big ground telescope, or invest in a satellite + rocket to get orbital observation?
- Real-world parallel: Hubble (optical), Chandra (X-ray), WISE/NEOWISE (IR), James Webb (IR/optical)

### Communication Relay Tiered Model

Three tiers of zone communication capability:

| Tier | Without Relay | Basic Relay | Advanced Relay |
|---|---|---|---|
| **Trade** | No trade in zone | Trade enabled (standard delay) | Trade enabled (reduced delay) |
| **Autopilot** | No automated operations | Autopilot enabled for zone | Full speed autopilot |
| **Ship commands** | Manual only (player-issued) | Full commands | Full commands + priority queue |
| **Data return** | Sensor data at 50% rate | Full rate | Full rate + bonus |
| **Scan sites** | Discoverable but not claimable | Fully operational | Fully operational |

**Implementation:** `zone_comm_tier(zone_id) -> CommTier` computed from deployed comm satellites. `CommTier::None | Basic | Advanced`. Systems check comm tier before allowing operations.

### Satellite Wear & Replacement

Satellites degrade over time (same WearState pattern as modules):
- Wear rate: content-defined per satellite type (e.g., 0.0001 per tick = ~10,000 tick lifetime = ~14 game-months at mpt=60)
- No maintenance (unlike station modules) — when worn out, satellite fails and must be replaced
- Creates ongoing manufacturing demand: need a steady supply chain of satellite replacements
- Failed satellites emit `SatelliteFailed` event, removed from active duty but stay in state (debris — future cleanup mechanic?)
- Autopilot tracks satellite health and queues replacement manufacturing/launches

### Deployment Mechanics

Two deployment paths (both use P2 launch system):

1. **From ground facility** — manufactured at ground, launched via Command::Launch with `LaunchPayload::Satellite(satellite_def_id, payload_config)`. P2 launch system handles transit to orbit. (Note: this requires adding the `Satellite` variant to `LaunchPayload` that was deferred from P2 per YAGNI — now it has a consumer.)

2. **From orbital station** — if a station exists, satellites in station inventory can be deployed directly via `Command::DeploySatellite { station_id, satellite_def_id, target_position }`. No rocket needed — direct orbital deployment. Cheaper but requires an existing station.

## Technical Approach

### Architecture

```
sim_core/src/types/satellite.rs (NEW)
  ├── SatelliteId (newtype)
  ├── SatelliteState { id, def_id, name, position, deployed_tick, wear, enabled, satellite_type, payload_config }
  └── CommTier enum { None, Basic, Advanced }

content/satellite_defs.json (NEW)
  ├── satellite types with: id, name, type, mass_kg, manufacturing_recipe, wear_rate, required_tech
  ├── behavior_config per type (scan_rate, comm_tier, nav_bonus, sensor_payload)
  └── content-driven: new types addable via JSON

sim_core/src/satellite.rs (NEW)
  ├── tick_satellites() — per-type behavior evaluation, wear accumulation
  ├── zone_comm_tier() — compute communication tier for a zone from deployed satellites
  └── zone_nav_bonus() — compute travel time reduction for a zone

GameState additions:
  pub satellites: BTreeMap<SatelliteId, SatelliteState>
```

### Tick Integration

Satellite ticking slots after station modules, before research:

```
... existing tick steps ...
4.  Tick stations (existing)
4.5 Tick ground facilities (P2)
4.6 Tick satellites (NEW) — per-type behavior, wear accumulation
5.  Advance research (existing — now includes orbital sensor data from science sats)
5.5 Evaluate milestones (P1)
...
```

### Zone System Integration

Satellites affect zone-level properties. Computed lazily per tick from deployed satellite positions:

- `zone_comm_tier(zone_id, satellites)` — scans all comm satellites in zone, returns highest tier
- `zone_nav_bonus(zone_id, satellites)` — sums nav satellite bonuses (diminishing returns)
- `zone_survey_rate(zone_id, satellites)` — survey satellites contribute to scan site discovery rate
- These functions called by trade system (comm check), ship transit (nav bonus), and scan site replenishment (survey rate)

## Implementation Tickets

### Ticket 1: SatelliteState entity + content schema

**What:** Define SatelliteState, SatelliteId, SatelliteDef content schema. Add satellites BTreeMap to GameState.

**Details:**
- SatelliteState struct with all fields (id, def_id, position, wear, enabled, satellite_type, payload_config)
- SatelliteId newtype
- content/satellite_defs.json with 4 satellite type definitions
- SatelliteDef: id, name, satellite_type (String), mass_kg, wear_rate, required_tech, behavior_config (type-specific)
- Load in sim_world::load_content()
- GameState.satellites: BTreeMap<SatelliteId, SatelliteState> (default empty, backward compatible)
- Serialization round-trips

**Acceptance criteria:**
- [ ] SatelliteState compiles and serializes
- [ ] 4 satellite defs in content
- [ ] GameState backward compatible (empty satellites map)
- [ ] Schema documented in docs/reference.md

**Dependencies:** None
**Size:** Medium

---

### Ticket 2: Satellite tick behavior — per-type evaluation

**What:** tick_satellites() function evaluating each satellite's type-specific behavior. Survey sats discover scan sites, comm sats enable zones, nav sats reduce transit, science sats generate data.

**Details:**
- New tick step 4.6 after ground facilities, before research
- Dispatch on satellite_type string (content-driven, not enum match)
- **Survey:** calls into scan site discovery with orbital bonus rate
- **Communication:** updates zone_comm_tier cache (computed from all comm sats)
- **Navigation:** updates zone_nav_bonus cache
- **Science:** generates sensor data (same generate_data() path as ground sensors, but with orbital multiplier)
- Wear accumulated each tick (wear += wear_rate)
- Disabled or worn-out (wear >= 1.0) satellites skip behavior
- Deterministic: iterate satellites sorted by ID

**Acceptance criteria:**
- [ ] Each satellite type produces its expected effect
- [ ] Survey sats contribute to scan site discovery
- [ ] Science sats generate data using sensor data kinds (from P2 Ticket 0b)
- [ ] Wear accumulates per tick
- [ ] Worn-out satellite stops producing effects
- [ ] Integration test: 4 satellite types all producing effects over 500 ticks

**Dependencies:** Ticket 1
**Size:** Large

---

### Ticket 3: Communication relay tiered zone gating

**What:** CommTier system — zones have communication capability based on deployed comm satellites. Trade, autopilot, and ship commands check comm tier.

**Details:**
- CommTier enum: None, Basic, Advanced (engine mechanic, not content)
- zone_comm_tier(zone_id) computed from comm satellites in or near zone
- Trade system checks: zone must have CommTier::Basic+ for trade commands
- Autopilot checks: zone must have CommTier::Basic+ for automated operations
- Ship commands: manual commands work without relay, automated routing requires Basic+
- Near-Earth zones (earth_orbit_zone, earth_neos) have implicit CommTier::Advanced (ground comms suffice)
- Distant zones (belt, outer) require deployed comm satellites
- SatelliteDef for comm satellite specifies which tier it provides

**Acceptance criteria:**
- [ ] Near-Earth zones always have Advanced comm (no satellite needed)
- [ ] Distant zones have None comm by default
- [ ] Deploying comm satellite upgrades zone comm tier
- [ ] Trade commands rejected in None-comm zones
- [ ] Autopilot skips None-comm zones for operations
- [ ] Unit test: comm tier computation from satellite positions
- [ ] Integration test: deploy comm sat → zone unlocks for trade

**Dependencies:** Ticket 2
**Size:** Medium-Large

---

### Ticket 4: Navigation beacon zone bonus

**What:** Navigation satellites reduce ship transit time in their deployed zone.

**Details:**
- zone_nav_bonus(zone_id) returns transit time multiplier (e.g., 0.85 = 15% faster)
- Multiple nav sats stack with diminishing returns: 1 sat = 15%, 2 sats = 22%, 3 sats = 27% (sqrt scaling)
- travel_ticks() checks zone_nav_bonus for origin and destination zones
- Bonus applies to both directions of travel through the zone

**Acceptance criteria:**
- [ ] Nav satellite reduces transit time in zone
- [ ] Multiple nav sats stack with diminishing returns
- [ ] travel_ticks() applies nav bonus
- [ ] Unit test: transit time with/without nav satellite
- [ ] Integration test: ship transit faster with nav beacon deployed

**Dependencies:** Ticket 2
**Size:** Small-Medium

---

### Ticket 5: Satellite manufacturing recipes

**What:** Assembler recipes for producing satellites at ground facilities or orbital stations. Different satellite types require different components.

**Details:**
- Recipes in content/recipes.json:
  - survey_satellite: bought: solar_cell + bought: camera_module + manufactured: guidance_board → survey_satellite
  - comm_relay_basic: bought: antenna_array + bought: solar_cell + manufactured: transponder → comm_relay
  - nav_beacon: bought: atomic_clock + bought: solar_cell + manufactured: signal_processor → nav_beacon
  - science_platform: manufactured: sensor_payload + bought: solar_array + manufactured: attitude_controller → science_satellite (sensor_payload varies by desired sensor type)
- Satellite items are inventory items (like rocket components) with type metadata
- Recipes gated by tech requirements
- Ground facility assembler produces satellites using bought + manufactured inputs

**Acceptance criteria:**
- [ ] 4+ satellite recipes defined in content
- [ ] Assembler produces satellite items
- [ ] Tech gating works
- [ ] Integration test: manufacture survey satellite from bought parts

**Dependencies:** Ticket 1 (satellite defs for recipe targets)
**Size:** Medium

---

### Ticket 6: Satellite deployment — ground launch + orbital deploy

**What:** Two deployment paths: launch from ground (via P2 launch system) and direct deploy from orbital station.

**Details:**
- **Ground launch:** Add `LaunchPayload::Satellite { satellite_def_id, payload_config }` variant to P2's LaunchPayload enum. When launch arrives at destination, creates SatelliteState at position. (This was deferred from P2 — now has a consumer.)
- **Orbital deploy:** `Command::DeploySatellite { station_id, satellite_item, target_position }`. Removes satellite from station inventory, creates SatelliteState. No rocket needed. Must be at same orbital zone.
- Both paths emit `Event::SatelliteDeployed { satellite_id, position, satellite_type }`
- Validation: satellite item in inventory, destination is valid orbital position

**Acceptance criteria:**
- [ ] Ground launch creates satellite at orbital destination
- [ ] Orbital deploy creates satellite from station inventory
- [ ] SatelliteDeployed event emitted
- [ ] Validation prevents invalid deployment
- [ ] Unit test: both deployment paths
- [ ] Integration test: end-to-end ground manufacture → launch → satellite operational

**Dependencies:** P2 launch system (VIO-557), Ticket 1, Ticket 5
**Size:** Medium

---

### Ticket 7: Satellite wear lifecycle + failure events

**What:** Satellites degrade over time. Failed satellites stop functioning and emit events. No repair — must be replaced.

**Details:**
- Wear accumulates per tick: `wear += def.wear_rate` (content-defined per satellite type)
- At wear >= 1.0: satellite auto-disabled, emit `Event::SatelliteFailed { satellite_id, satellite_type }`
- Failed satellite remains in state (position tracked) but produces no effects
- Wear rate varies by type: survey sats (cheaper, shorter-lived) vs comm relays (more expensive, longer-lived)
- FE handler for SatelliteFailed event (per event sync)
- No maintenance mechanic — replacement is the only option

**Acceptance criteria:**
- [ ] Wear accumulates correctly per tick
- [ ] Satellite disables at wear >= 1.0
- [ ] SatelliteFailed event emitted
- [ ] Failed satellite produces no zone effects
- [ ] FE event handler exists
- [ ] ci_event_sync.sh passes
- [ ] Unit test: satellite lifecycle from deployment to failure

**Dependencies:** Ticket 2
**Size:** Small-Medium

---

### Ticket 8: Satellite events + FE handlers

**What:** New Event variants for satellite lifecycle. FE handlers per event sync rule.

**Details:**
- New events:
  - `SatelliteDeployed { satellite_id, position, satellite_type }`
  - `SatelliteFailed { satellite_id, satellite_type }`
  - `CommTierChanged { zone_id, old_tier, new_tier }` (when comm satellite changes zone capability)
- FE handlers in applyEvents.ts
- SSE streaming includes satellite events
- ci_event_sync.sh updated

**Acceptance criteria:**
- [ ] 3 new Event variants compile and serialize
- [ ] applyEvents.ts handles all 3
- [ ] ci_event_sync.sh passes
- [ ] SSE stream includes satellite events

**Dependencies:** Ticket 6 (deploy events), Ticket 7 (failure events)
**Size:** Small

---

### Ticket 9: Autopilot satellite management

**What:** Autopilot evaluates coverage needs and manages satellite lifecycle — manufacturing, deployment, replacement.

**Details:**
- For GroundFacilityAgent: new SatelliteConcern that evaluates:
  - Which zones need survey coverage (no survey sat = can't discover asteroids there)
  - Which zones need comm coverage (needed before any operations)
  - When to manufacture replacement satellites (aging satellite approaching failure)
- For StationAgent (once stations exist): deploy satellites from station inventory
- Priority: comm relay first (enables zone), then survey (enables discovery), then nav (optimization), then science (data bonus)
- Budget-aware: satellite manufacturing and launch costs factored into budget allocation

**Acceptance criteria:**
- [ ] Autopilot identifies zones needing satellite coverage
- [ ] Manufactures appropriate satellite types
- [ ] Schedules launches for deployment
- [ ] Replaces aging satellites before failure
- [ ] Integration test: autopilot achieves zone coverage within expected timeframe

**Dependencies:** Ticket 6 (deployment), P2 autopilot (VIO-554, VIO-562)
**Size:** Medium-Large

---

### Ticket 10: Satellite milestones

**What:** Progression milestones for satellite operations, extending P1/P2 milestone system.

**Details:**
- New milestones in content/milestones.json:
  - `first_satellite_deployed` — any satellite reaches orbit → $15M grant
  - `first_comm_relay` — comm satellite deployed, zone enabled → $10M grant
  - `orbital_survey_network` — 3+ survey satellites across 2+ zones → $25M grant
  - `deep_space_comm` — comm relay in belt zone → $30M grant + unlock belt operations
  - `science_constellation` — 2+ science satellites operational → $20M grant

**Acceptance criteria:**
- [ ] 5 satellite milestones defined
- [ ] Milestones trigger correctly
- [ ] Grants fund continued operations
- [ ] Integration test: autopilot reaches first_satellite_deployed from ground_start

**Dependencies:** P1 milestone system (VIO-533)
**Size:** Small-Medium

---

### Ticket 11: sim_bench satellite scenarios

**What:** Scenarios validating satellite operations end-to-end.

**Details:**
- `scenarios/satellite_deployment.json` — ground_start, 5000 ticks, 20 seeds. First satellite manufactured + launched + operational.
- `scenarios/satellite_coverage.json` — ground_start, 15000 ticks, 10 seeds. Comm relay enables zone, survey network covers 2+ zones.
- `scenarios/satellite_lifecycle.json` — 20000 ticks, 10 seeds. Satellite wear → failure → replacement cycle.
- CI smoke includes quick satellite check.

**Acceptance criteria:**
- [ ] Satellite deployment scenario passes
- [ ] Coverage scenario validates zone enablement
- [ ] Lifecycle scenario validates replacement cycle
- [ ] CI smoke includes satellite check

**Dependencies:** Ticket 9 (autopilot manages satellites), Ticket 10 (milestones)
**Size:** Medium

---

### Ticket 12: Score dimension extension for satellites

**What:** P0 scoring dimensions account for satellite activity.

**Details:**
- Expansion dimension: satellite count contributes (alongside stations and fleet)
- Research dimension: science satellite data production bonus
- Fleet Operations: satellite deployments count as mission completions
- Efficiency: satellite utilization (active vs failed ratio)

**Acceptance criteria:**
- [ ] Satellite activity contributes to scoring dimensions
- [ ] Score trajectory shows improvement when satellites deployed
- [ ] Comparison: ground_start with satellites vs without

**Dependencies:** P0 scoring (VIO-521)
**Size:** Small

---

## Dependency Graph

```
Ticket 1 (entity + schema) ──→ Ticket 2 (tick behavior) ──→ Ticket 3 (comm tiered gating)
       │                              │                              │
       │                              ├──→ Ticket 4 (nav bonus)      │
       │                              │                              │
       │                              └──→ Ticket 7 (wear lifecycle) │
       │                                                             │
       └──→ Ticket 5 (manufacturing) ──→ Ticket 6 (deployment) ─────┘
                                              │                      │
                                         Ticket 8 (events + FE)     │
                                              │                      │
                                         Ticket 9 (autopilot) ──────┘
                                              │
                                    Ticket 10 (milestones)
                                              │
                                    Ticket 11 (scenarios)
                                              │
                                    Ticket 12 (scoring)
```

**Critical path:** 1 → 2 → 3 → 9 → 10 → 11 (entity → behavior → comm gating → autopilot → milestones → validation)

**Parallel streams:**
- Stream A: 1 → 2 → 3, 4, 7 (entity → behaviors)
- Stream B: 1 → 5 → 6 → 8 (manufacturing → deployment → events)
- Convergence: 9 needs both streams

## P2 Integration Points

| P2 Delivers | P4 Uses It For |
|---|---|
| Launch system (VIO-557) | Ground-to-orbit satellite deployment |
| GroundFacilityAgent (VIO-554) | Satellite manufacturing + launch planning |
| Sensor data kinds (VIO-544, VIO-552) | Science satellite payload configuration |
| Rocket manufacturing (VIO-559) | Satellite launch vehicles |
| Ground milestones (VIO-564) | first_satellite_deployed milestone follows first_launch |

**P2 ticket update needed:** Add `LaunchPayload::Satellite` variant to VIO-557 when P4 implementation begins (was deferred per YAGNI, now has a consumer).

## Risk Analysis

### Medium Risk

**Satellite busywork** — if deploying and replacing satellites feels like tedious maintenance rather than strategic infrastructure.
- *Mitigation:* Autopilot handles routine satellite management. Player's decisions are strategic (which zones to cover), not tactical (individual satellite deployment).

**Zone comm gating frustration** — players can't do anything in a zone without deploying a comm satellite first.
- *Mitigation:* Tiered approach (basic manual operations possible without relay). Near-Earth zones have implicit comms. First comm satellite is cheap and launchable early.

### Low Risk

**Performance with many satellites** — dozens of satellites ticking each tick.
- *Mitigation:* Satellites are simple (no modules, no inventory, no power computation). Tick should be <1% overhead even with 50 satellites.

## Sources & References

### Origin

- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 4 section. Key departures: content-driven types (not enum), tiered comm (not hard gate), science platform type added.

### Internal References

- P2 launch system: VIO-557 (launch command), VIO-559 (rocket manufacturing)
- P2 sensors: VIO-544 (DataKind strings), VIO-551 (sensor types), VIO-552 (data kinds)
- P1 milestones: VIO-533 (evaluation engine)
- Existing WearState pattern: crates/sim_core/src/types/state.rs (module wear)
- Event sync: scripts/ci_event_sync.sh

### Related Linear

- VIO-506: Plan P4 (planning ticket — mark done)
- P2 project: VIO-543 through VIO-566
