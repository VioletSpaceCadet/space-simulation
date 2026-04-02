# Crew System Design

**Goal:** Typed crew pools as a resource constraint on module and ship operations, with recruitment via trade, autopilot assignment, and automated module variants as the core progression mechanic.
**Status:** Planned
**Linear Project:** [Crew System](https://linear.app/violetspacecadet/project/crew-system-e8d9ced785ea)

## Overview

All operations in the simulation are fully automated with no labor dimension. Expansion is purely material-constrained — if you have ore and power, you can run unlimited modules. Adding crew as a resource creates a second bottleneck that forces strategic decisions about where to allocate people, what to automate, and when to expand workforce.

Crew are typed pools (Operator, Technician, Scientist, Pilot) — numeric counts, not named individuals. Modules have crew requirements defined in content JSON. Unstaffed modules don't run (binary, same as power stalling). Crew is recruited via the existing import/trade system. The autopilot assigns crew to modules based on a unified module priority field.

The core progression mechanic is automation: early game is crew-dependent, research unlocks automated module variants (higher material cost, lower output), late game frees crew for specialized roles. This is "you need crew to run things, then research removes that need" — fundamentally different from crew-as-bonus.

The system is designed with a future two-tier population model in mind. Phase 1 is crew as commodity (Tier 1). Tier 2 — leaders as named individuals with traits, emerging from crew pools — is architecturally accommodated via `leaders: Vec<LeaderId>` fields on station and ship state (empty in Phase 1, costs nothing, documents intent). The leader system is not designed here, but the crew system doesn't close the door on it.

## Design

### Data Model

#### New newtypes

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CrewRole(pub String);  // Content-driven, like SlotType

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LeaderId(pub String);  // Placeholder for future leader system
```

CrewRole is a content-driven string newtype. Adding a new role = JSON change only. LeaderId is a future seam — empty in Phase 1.

#### CrewRoleDef (content-driven)

```rust
pub struct CrewRoleDef {
    pub id: CrewRole,
    pub name: String,
    pub recruitment_cost: f64,
}
```

Loaded from `content/crew_roles.json`. 4 initial roles: Operator, Technician, Scientist, Pilot.

#### StationState additions

```rust
pub struct StationState {
    // ... existing ...
    pub crew: BTreeMap<CrewRole, u32>,     // Total crew at station (assigned + unassigned)
    pub leaders: Vec<LeaderId>,            // Empty in Phase 1, future leader seam
}
```

BTreeMap for deterministic iteration. `crew` represents total headcount — available crew is computed by subtracting all module assignments. One source of truth for total count.

Available crew helper:
```rust
fn available_crew(station: &StationState, role: &CrewRole) -> u32 {
    let total = station.crew.get(role).copied().unwrap_or(0);
    let assigned: u32 = station.modules.iter()
        .map(|m| m.assigned_crew.get(role).copied().unwrap_or(0))
        .sum();
    total.saturating_sub(assigned)
}
```

#### ModuleState additions

```rust
pub struct ModuleState {
    // ... existing ...
    pub assigned_crew: BTreeMap<CrewRole, u32>,  // Crew assigned to this module
    #[serde(skip)]
    pub crew_satisfied: bool,                    // Derived state, recomputed each tick
    pub module_priority: u32,                    // RENAMED from manufacturing_priority
}
```

`crew_satisfied` is `#[serde(skip)]`, default false, recomputed each tick before module execution. Initialized correctly during state load (run satisfaction check once before first tick) to avoid spurious transition events.

`module_priority` replaces `manufacturing_priority` — one field for all resource contention (inventory consumption order, crew assignment, power). Higher = served first, ID tiebreak. `#[serde(alias = "manufacturing_priority")]` for backward compatibility — old saves load correctly, new saves write `module_priority`. Serde aliases are read-only (deserialization only), which is exactly right.

#### ModuleDef additions

```rust
pub struct ModuleDef {
    // ... existing ...
    pub crew_requirement: BTreeMap<CrewRole, u32>,  // Required crew to operate
}
```

`#[serde(default)]` → empty map. Existing modules have no crew requirement — they run as before. Automated module variants explicitly set `crew_requirement: {}`.

#### ShipState additions

```rust
pub struct ShipState {
    // ... existing ...
    pub crew: BTreeMap<CrewRole, u32>,     // Crew aboard ship
    pub leaders: Vec<LeaderId>,            // Empty in Phase 1
}
```

Ship crew requirements are on HullDef (designed in hull+slot project). Implementation of ship crew check is blocked by hull+slot landing.

#### HullDef additions (coordinated with hull+slot project)

```rust
pub struct HullDef {
    // ... existing ...
    pub crew_requirement: BTreeMap<CrewRole, u32>,  // Required crew to operate
    pub passenger_capacity: u32,                    // For Phase 2 crew transfers
}
```

Both `#[serde(default)]`. Phase 1 hull content sets `passenger_capacity: 0` (documented as "Phase 2: crew transfers"). `crew_requirement` designed but check deferred until hull+slot lands.

#### TradeItemSpec extension

```rust
pub enum TradeItemSpec {
    // ... existing ...
    Crew { role: CrewRole, count: u32 },  // NEW
}
```

Crew imported via existing `Command::Import`. Export of crew explicitly rejected at command processing time — business rule enforced in the export handler.

#### GameContent addition

```rust
pub struct GameContent {
    // ... existing ...
    pub crew_roles: BTreeMap<CrewRole, CrewRoleDef>,
}
```

#### ModifierSource addition

```rust
pub enum ModifierSource {
    // ... existing ...
    Crew(CrewRole),  // Future: crew-derived bonuses (not used in Phase 1)
}
```

Added for forward compatibility. Phase 1 crew is binary (staffed or not). Future: Scientists in labs apply ResearchSpeed bonus via this source.

#### Commands

```rust
Command::AssignCrew {
    station_id: StationId,
    module_id: ModuleInstanceId,
    role: CrewRole,
    count: u32,
}

Command::UnassignCrew {
    station_id: StationId,
    module_id: ModuleInstanceId,
    role: CrewRole,
    count: u32,
}

Command::SetModulePriority {  // RENAMED from SetManufacturingPriority
    station_id: StationId,
    module_id: ModuleInstanceId,
    priority: u32,
}
```

AssignCrew validation: station exists, module exists on station, role in crew_roles content, available crew >= count, module's crew_requirement includes this role.

#### Events

```rust
Event::CrewAssigned { station_id, module_id, role, count }
Event::CrewUnassigned { station_id, module_id, role, count }
Event::ModuleUnderstaffed { station_id, module_id, role, required, assigned }
Event::ModuleFullyStaffed { station_id, module_id }
```

ModuleUnderstaffed/FullyStaffed emit on transition only (not every tick), same pattern as ModuleStalled/ModuleResumed.

### Tick Ordering

No new tick phase. Crew satisfaction is checked within the existing module tick loop (step 3):

```rust
// Before ticking each module:
let crew_satisfied = module_def.crew_requirement.iter().all(|(role, required)| {
    module.assigned_crew.get(role).copied().unwrap_or(0) >= *required
});
module.crew_satisfied = crew_satisfied;

// Emit events on transition only
if !crew_satisfied && was_satisfied { events.push(ModuleUnderstaffed { ... }); }
if crew_satisfied && !was_satisfied { events.push(ModuleFullyStaffed { ... }); }

// Skip module if not staffed
if !crew_satisfied { continue; }
```

Modules with empty `crew_requirement` always satisfy the check — existing modules run unchanged.

`crew_satisfied` initialized correctly during state load (run satisfaction check once before first tick) to suppress spurious transition events on first tick after load.

### Autopilot

**CrewAssignmentBehavior** — runs every tick, per station:
1. Compute available crew per role
2. Sort modules by `module_priority` (desc) then `id` (asc)
3. For each understaffed module (in priority order), assign available crew
4. Rebalancing guard: only steal crew from modules with **strictly lower** priority. Equal priority = stable (ID tiebreak determines permanent winner). Prevents oscillation.
5. Generate AssignCrew/UnassignCrew commands through normal command queue (one-tick latency, same as all autopilot behaviors)

**CrewRecruitmentBehavior** — runs every tick, per station:
1. Count total crew demand vs supply per role
2. If demand > supply and balance permits, import crew
3. Priority: recruit the role that would **unstall the most modules** (by impact, not by gap count)
4. Generate `Import { Crew { role, count } }` commands

### SSE / API / Frontend Integration

**`GET /api/v1/content`:** Crew role catalog served.

**`GET /api/v1/state`:** StationState includes `crew`, ModuleState includes `assigned_crew`, `crew_satisfied`.

**No new endpoints.**

**Frontend:**
- `applyEvents.ts` — handlers for CrewAssigned, CrewUnassigned, ModuleUnderstaffed, ModuleFullyStaffed
- `eventSchemas.ts` — Zod schemas for new events
- `types.ts` — crew fields on StationState, ModuleState, ShipState
- `ModuleCard.tsx` — UNDERSTAFFED badge alongside existing STALLED/OVERHEAT badges. Assigned crew counts.
- `StationDetail.tsx` — crew roster summary (role counts, available vs assigned)
- `ci_event_sync.sh` — add new Event variants
- Phase 1: read-only display. Crew assignment UI deferred to mockup pass.

### Content Files

**New file: `content/crew_roles.json`**

```json
[
  { "id": "operator",   "name": "Operator",   "recruitment_cost": 50000.0 },
  { "id": "technician", "name": "Technician", "recruitment_cost": 75000.0 },
  { "id": "scientist",  "name": "Scientist",  "recruitment_cost": 100000.0 },
  { "id": "pilot",      "name": "Pilot",      "recruitment_cost": 80000.0 }
]
```

**Updated: `content/module_defs.json`** — crew requirements:

| Module | Crew Requirement |
|---|---|
| `module_basic_iron_refinery` | 3 operator |
| `module_basic_smelter` | 2 operator, 1 technician |
| `module_basic_assembler` | 2 operator |
| `module_shipyard` | 2 operator, 1 technician |
| `module_maintenance_bay` | 2 technician |
| `module_lab_*` (all labs) | 2 scientist |
| `module_sensor_array` | 1 operator |
| `module_solar_array` | 0 (passive) |
| `module_battery` | 0 (passive) |
| `module_radiator` | 0 (passive) |
| `module_storage` | 0 (passive) |

**New automated module variants:**

| Module | Crew | Trade-offs | Tech Gate |
|---|---|---|---|
| `module_automated_refinery` | 0 | 2x material cost, -15% ProcessingYield | `tech_automation_basic` |
| `module_automated_assembler` | 0 | 2x material cost, +50% AssemblyInterval (slower) | `tech_automation_basic` |

Phase 1 trade-offs are intentionally varied (quality vs speed). Note for future: if scaling to 8+ automated variants, converge on a consistent trade-off theme.

**New tech:** `tech_automation_basic` — gates automated module variants.

**Updated: `content/dev_advanced_state.json`** — starting crew roster: 10 operators, 4 technicians, 2 scientists, 2 pilots. Slight operator surplus so the player experiences the system working before hitting the constraint.

**Updated: `content/pricing.json`** — crew role pricing (importable: true, exportable: false).

**Content validation at load time:**
- Crew role ID uniqueness — panic
- Module crew_requirement references valid crew roles — panic
- Recruitment cost > 0 — warning

### Migration / Backwards Compatibility

- `StationState.crew` — `#[serde(default)]` → empty. No crew = no requirements checked.
- `StationState.leaders` — `#[serde(default)]` → empty vec.
- `ModuleState.assigned_crew` — `#[serde(default)]` → empty.
- `ModuleState.crew_satisfied` — `#[serde(skip)]`, recomputed each tick.
- `ModuleState.module_priority` — `#[serde(default)]` with `#[serde(alias = "manufacturing_priority")]`. Old saves load correctly. New saves write `module_priority`.
- `ModuleDef.crew_requirement` — `#[serde(default)]` → empty. Existing modules run unchanged.
- `ShipState.crew` / `leaders` — `#[serde(default)]` → empty.
- `TradeItemSpec::Crew` — new variant. Existing trade commands unaffected.
- `Command::SetModulePriority` — new name. `SetManufacturingPriority` kept as alias.

**No breaking changes to saves.** Old saves load with no crew, no requirements, everything runs as before.

## Testing Plan

- **Unit tests** (sim_core): crew role loading + validation, AssignCrew/UnassignCrew command processing (valid/invalid: insufficient crew, role not in requirement), crew satisfaction check (staffed → runs, understaffed → skips), transition events (ModuleUnderstaffed/ModuleFullyStaffed), Import crew via TradeItemSpec::Crew, Export crew rejected, empty crew_requirement → always satisfied, available crew computation, crew_satisfied initialization on load (no spurious events)
- **Rebalancing oscillation test** (critical): two modules same priority competing for same role → ID tiebreak, stable assignment, no oscillation. Higher priority module always wins. Fully staffed module never has crew pulled by equal-or-lower priority.
- **Automated vs crewed benchmark** (critical): station with automated refinery produces measurably less than identical station with crewed refinery over same tick window. Proves the trade-off is real, not just defined in content.
- **Integration test**: full lifecycle — import crew → autopilot assigns → modules run → uninstall module → crew returns to pool → autopilot reassigns
- **Determinism regression**: same seed twice, identical crew assignments and module states
- **sim_bench scenario**: `scenarios/crew_system.json` — all crewed modules staffed by tick 500, recruitment imports when demand > supply, automated modules run without crew after tech unlock, automated modules produce less than crewed equivalents
- **Frontend**: vitest for Zod schemas, crew display components

## Ticket Breakdown

### Crew System

1. **CR-01: Data model + content loading** — CrewRole/LeaderId newtypes, CrewRoleDef, crew_roles.json loading + BTreeMap on GameContent, ModuleDef.crew_requirement, ModifierSource::Crew, module_priority rename (with serde alias), SetModulePriority command rename, content validation
2. **CR-02: Crew state + staffing logic** — StationState.crew/leaders, ModuleState.assigned_crew/crew_satisfied, ShipState.crew/leaders, AssignCrew/UnassignCrew commands + validation, crew satisfaction check in tick loop, transition events, Export rejection for Crew, crew_satisfied initialization on load
3. **CR-03: Crew recruitment via trade** — TradeItemSpec::Crew variant, Import handling for crew, pricing.json crew entries
4. **CR-04: Autopilot crew behaviors** — CrewAssignmentBehavior (priority-based, rebalancing guard, oscillation prevention), CrewRecruitmentBehavior (import by impact not gap count)
5. **CR-05: Crew content** — Crew requirements on all module defs, 2 automated module variants with trade-off modifiers, tech_automation_basic, dev_advanced_state.json crew roster (10 op, 4 tech, 2 sci, 2 pilot)
6. **CR-06: SSE + frontend data layer** — Zod schemas for crew events, applyEvents handlers, types.ts crew fields, ModuleCard UNDERSTAFFED badge, StationDetail crew roster, ci_event_sync.sh
7. **CR-07: Testing + determinism validation** — Unit tests, rebalancing oscillation test, automated-vs-crewed benchmark, integration test, determinism regression, sim_bench crew_system.json scenario

Dependencies: CR-01 → CR-02, CR-03, CR-05; CR-02 + CR-03 → CR-04; CR-02 + CR-05 → CR-06; all → CR-07

## Open Questions

- **Leader system**: Phase 1 doesn't design it. `leaders: Vec<LeaderId>` is the seam. Leaders emerge from crew pools — a Scientist assigned to a lab for N ticks becomes a leader candidate. Traits, quirks, names are content-driven JSON. The modifier system handles leader bonuses (ModifierSource::Leader). The event system records their history. Not Phase 1.
- **Crew bonuses (ModifierSource::Crew)**: Phase 1 crew is binary (staffed or not). Future: Scientists in labs could apply ResearchSpeed bonus. The variant exists for forward compatibility but isn't wired.
- **Passenger capacity**: Field on HullDef, set to 0 in Phase 1 content. Phase 2: crew transfers use passenger slots on transport ships.
- **Crew numbers per module**: Starting values are educated guesses. Needs balance tuning via sim_bench after the system is stable.
- **Habitat modules**: Phase 2. Housing capacity limits max crew per station. Phase 1 has no crew cap.
- **Organic population growth**: Phase 2+. Phase 1 recruitment is import-only.
- **Automated trade-off consistency**: Phase 1 has varied trade-offs (quality vs speed). At scale (8+ variants), converge on a consistent theme.

## Manufacturing DAG Coordination

The Manufacturing DAG project (VIO-369–374) uses `manufacturing_priority` which this project renames to `module_priority`. Coordination needed:
- `SetManufacturingPriority` command → `SetModulePriority` (with alias for backward compat)
- Manufacturing DAG design doc updated to use `module_priority`
- Whichever project implements first does the rename; the other uses the new name
