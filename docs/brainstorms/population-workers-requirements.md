---
date: 2026-03-21
topic: population-workers-crew
---

# Population / Workers / Crew System

## Problem Frame

All operations in the simulation are fully automated with no labor dimension. Expansion is purely material-constrained — if you have ore and power, you can run unlimited modules. Adding crew as a resource creates a second bottleneck that forces strategic decisions about where to allocate people, what to automate, and when to expand workforce. Automation becomes a tech progression axis: early game is crew-dependent, late game frees crew for specialized roles through research.

## Requirements

### Crew as a Resource

- R1. **Typed crew pools** per station/ship. Crew roles: Operator, Technician, Scientist, Pilot. Each role is a numeric count (not named individuals in Phase 1). Example: "Station Alpha has 8 Operators, 3 Technicians, 2 Scientists."
- R2. **Modules have crew requirements** defined in content JSON. A basic refinery requires 3 Operators. A research lab requires 2 Scientists. Unstaffed modules do not run. Crew requirement is per-module-def, not per-module-type.
- R3. **Ships have crew requirements** based on hull class. A Mining Barge requires 2 Pilots + 1 Operator. A Transport Hauler requires 1 Pilot. Uncrewed ships cannot operate.
- R4. **Automated module variants** unlocked via research. An automated refinery has crew_required: 0 but costs more materials and has lower base efficiency. Creates a meaningful trade-off: cheap+crewed vs expensive+automated. This is the core progression mechanic.

### Automation Progression Arc

- R5. **Early game:** Most modules require crew. Crew is the primary expansion bottleneck. You have limited population and must choose which modules to staff.
- R6. **Mid game:** Research unlocks automated variants of basic modules (automated refinery, automated assembler). Crew freed from operations shifts to labs and specialized roles.
- R7. **Late game:** Most industrial operations automated. Remaining crew are specialists — scientists directing research, commanders providing station bonuses (→ leader system), explorers investigating anomalies (→ artifact system).
- R8. **Automated modules have trade-offs**, not just "strictly better." Options: higher material cost, higher power draw, higher wear rate, lower quality output, or no hull/station bonus interaction. Content-tunable per module def.

### Crew Logistics

- R9. **Recruitment via trade system.** Hire crew through the existing import mechanism. Each role has a recruitment cost in the pricing table. Crew arrives at the station that placed the order (imported like any other item).
- R10. **Physical transit required** for crew transfers between stations. Crew must ride ships with passenger capacity. Creates demand for transport-class ships and makes crew assignment a logistics decision, not just a menu click.
- R11. **Ship hull classes define passenger capacity.** Transport Hauler has high passenger capacity. Mining Barge has minimal (just its own crew). This feeds back into the entity depth system — hull design includes crew quarters.
- R12. **Crew assignment** at the station level. Crew are assigned to specific modules (or remain in a station's unassigned pool). Autopilot handles assignment based on module priority (same pattern as power priority).

### Crew State

- R13. **Per-station crew roster:** `HashMap<CrewRole, u32>` for assigned + unassigned counts. Serializable for save/load.
- R14. **Per-module crew assignment:** Each module tracks how many crew of which roles are currently assigned. Module runs only if crew requirement met.
- R15. **Per-ship crew manifest:** Ships track their crew (role counts). Must meet hull crew requirement to operate.

## Success Criteria

- Expansion is crew-gated in early game — can't run 10 refineries with 3 operators
- Autopilot makes crew assignment decisions (prioritizes most impactful modules)
- Research into automation visibly frees up crew for reallocation
- Transport ships have a reason to exist (crew logistics)
- Adding a new crew role = content JSON change only (role name + recruitment cost)

## Scope Boundaries

- **Not in scope:** Named individuals, traits, experience, morale, skill levels (→ leader system brainstorm)
- **Not in scope:** Crew death, injury, or health mechanics
- **Not in scope:** Organic population growth, habitats, quality-of-life (→ Phase 2+)
- **Not in scope:** Immigration based on station quality (→ Phase 3)
- **Not in scope:** Crew training or role conversion (→ future, ties into leader emergence)
- **Design for future:** Crew pool structure should accommodate later addition of named individuals (leaders emerge from typed pools)

## Key Decisions

- **Automation as tech progression, not efficiency slider:** You don't add crew for +10% bonus. You need crew to run things, then research removes that need. Fundamentally different incentive structure.
- **Typed pools, not individuals (Phase 1):** Keeps data simple. Leaders layer on top later as named characters drawn from or assigned to pools.
- **Physical transit, not teleport:** Crew logistics creates real transport demand and makes station placement matter for workforce planning.
- **Recruitment via trade (Phase 1):** Uses existing import system. Organic growth and immigration as future enhancements.

## Phasing

### Phase 1: Core Crew System
- Typed crew pools (Operator, Technician, Scientist, Pilot)
- Module crew requirements in content JSON
- Ship crew requirements in hull class defs
- Crew recruitment via trade/import
- Autopilot crew assignment (priority-based)
- 2-3 automated module variants (automated refinery, automated assembler)
- Passenger capacity on ship hulls

### Phase 2: Crew Logistics
- Crew transfer commands (move crew between stations via transport ships)
- Autopilot crew rebalancing across stations
- Habitat modules (housing capacity per station, limits max crew)
- Organic population growth (slow, habitat-dependent)

### Phase 3: Specialization
- Crew experience (pools accumulate experience, affecting efficiency)
- Leader emergence (experienced crew become named individuals with traits → leader system)
- Quality-of-life metrics affecting recruitment/retention
- Role conversion/training (retrain Operators as Technicians)

## Dependencies / Assumptions

- **Entity depth system** (hull+slot) should land first — crew requirements go on hull defs and module defs
- **StatModifier system** (VIO-332) — crew bonuses and automation trade-offs flow through it
- **Existing trade/import system** handles crew recruitment with no architecture changes
- **Autopilot** already has priority-based assignment patterns (power priority) that extend to crew

## Outstanding Questions

### Resolve Before Planning

(None — all blocking questions resolved)

### Deferred to Planning
- [Affects R2][Needs research] What crew numbers feel right per module? Needs content balancing pass alongside early-game economy.
- [Affects R10][Technical] How does passenger capacity interact with cargo capacity on ships? Shared volume? Separate?
- [Affects R12][Technical] Autopilot crew assignment heuristic — priority order for which modules get staffed first when crew is scarce?
- [Affects R4][Needs research] What are the right trade-offs for automated vs crewed modules? Higher cost only, or also lower output?

## Next Steps

→ `/ce:plan` for Phase 1 implementation planning (after entity depth lands).
