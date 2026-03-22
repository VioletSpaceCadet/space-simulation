---
date: 2026-03-21
topic: entity-depth-hull-slot-manufacturing
---

# Entity Depth: Hull+Slot Architecture & Manufacturing DAGs

## Problem Frame

The simulation currently has one ship type (mining shuttle) with no customization, and stations are flat bags of installable modules. Manufacturing chains are shallow (2 tiers max). This creates no strategic decision space — players/autopilot build "more of the same" rather than choosing *what* to build. Adding hull classes with module slots, deeper manufacturing chains, and a unified construction system creates the core progression and decision-making that transforms the sim from an idle loop into a real strategy game.

## Requirements

### Ship System

- R1. **Hull classes** define ship archetypes with base stats (mass, cargo capacity, power budget, thermal capacity, slot count/types) and **hull bonuses** (Eve Online-style). Hull bonuses are stat modifiers that make the same module perform differently in different hulls — a mining laser in a Mining Barge outperforms the same laser in a General Purpose hull. Bonuses can be flat (always active), scaling (improve with research tier), or conditional (only affect specific slot types or module categories). All bonuses flow through the StatModifier system (VIO-332). Example: Mining Barge gets +25% MiningRate (flat) + +5% MiningYield per mining tech level (scaling).
- R2. **Typed module slots** on each hull class. Slot types constrain which modules can be fitted (e.g., high-power slot for weapons/mining lasers, utility slot for scanners/cargo expanders, propulsion slot for engines). Initial slot types: propulsion, utility, industrial, defense (defense reserved for future).
- R3. **Ship modules** are items manufactured via the assembler/production system. Each module has stats that feed into the ship's physics model: mass (affects speed via ticks_per_au), power draw, thermal output, and stat modifiers for specific capabilities.
- R4. **Ship templates** (blueprints) define a hull class + module loadout. Templates are the unit of construction — the shipyard builds from a template. Both autopilot and future human players create/modify templates.
- R5. **Ship stats are computed from hull + fitted modules.** Speed derives from mass + propulsion. Cargo from hull base + cargo expander modules. Mining rate from hull bonus + mining module stats. All via the StatModifier pipeline.
- R6. **Initial hull classes (Phase 1, ~3-5 types):**
  - Mining Barge — hull bonus to mining rate, industrial + utility slots
  - Transport Hauler — large base cargo, utility + propulsion slots
  - Survey Scout — hull bonus to scan speed, utility slots, fast base speed
  - General Purpose — no hull bonus, balanced slots (current mining shuttle equivalent)
  - (Optional) Construction Vessel — for station deployment, Phase 2

### Station System

- R7. **Station frames** define station archetypes with base stats (power capacity, thermal capacity, storage capacity, slot count/types) and frame bonuses. Mirrors ship hull concept.
- R8. **Station slot types** constrain which modules can be installed: industrial (processors, assemblers), research (labs), utility (solar, battery, radiator, maintenance), structural (storage expansion, frame upgrades).
- R9. **Station frame upgrades** allow expanding a station over time — adding more slots, increasing base capacity. This is how stations "grow bigger." Upgrades are manufactured items installed via construction.
- R10. **Station templates** define a frame + module loadout for new station construction. Autopilot or player designs them.
- R11. **Station construction** requires deploying a station kit (manufactured item) at a location, then bootstrapping with modules. Phase 1: instant deployment from kit. Phase 2: construction ship required for remote deployment.
- R12. **Initial station frames (Phase 1, ~3-4 types):**
  - Outpost — small, 4-6 slots, low power/storage, cheap to build
  - Industrial Hub — medium, 8-12 slots, balanced (current station equivalent)
  - Research Station — frame bonus to research speed, more research slots
  - (Future) Shipyard Complex — large, many industrial slots, frame bonus to assembly speed

### Manufacturing DAG

- R13. **Modular production chain depth.** Simple products (repair kits) remain 2 tiers. Complex products (ship hulls, advanced modules) require 3-5 tiers. The production graph is a DAG defined entirely in content JSON.
- R14. **Intermediate products** bridge raw materials and final products. Examples: alloy plates (Fe + heat → steel plate), circuit boards (Si → refined silicon → circuit), structural beams (Fe → beam). These create meaningful production planning decisions.
- R15. **Alternative recipes** for the same output. Example: basic hull plating from Fe only (slow, low quality) vs advanced hull plating from steel alloy (faster, better stats). Creates research-driven manufacturing progression.
- R16. **Byproduct utilization.** Slag and waste heat are already modeled. New intermediates should generate byproducts that feed other chains (e.g., slag reprocessing for trace minerals, waste heat for thermal processes).
- R17. **Recipe unlocks via research.** Advanced recipes gated by tech tree. Basic versions available from start, optimized versions from research. Feeds the "research → better manufacturing → better ships/stations → more research" progression loop.
- R18. **All recipes, intermediates, and production chains defined in content JSON.** No new recipe types require code changes — only content additions. The existing Processor/Assembler module types handle all production; new recipes just add entries.

### Template & Blueprint System

- R19. **Templates are first-class game objects** — serializable, storable, shareable (future multiplayer consideration). A template is a hull/frame ID + ordered list of (slot_index, module_def_id) pairs.
- R20. **Template validation** at creation time — verify all modules fit their slot types, power budget doesn't exceed hull capacity, mass is within hull limits.
- R21. **Autopilot template selection** — autopilot evaluates fleet needs (mining capacity, transport capacity, scan coverage) and selects appropriate templates to build. Default templates provided in content for bootstrap.
- R22. **Template cost computation** — total material/component cost derived from hull recipe + all module recipes. Used by autopilot for build prioritization and by UI for cost display.

## Success Criteria

- Adding a new ship hull class = adding JSON content (hull def + recipe), no code changes
- Adding a new ship/station module = adding JSON content (module def + recipe), no code changes
- Adding a new manufacturing intermediate = adding JSON content (element + recipe), no code changes
- Fleet composition becomes heterogeneous — different ship types assigned different roles
- Manufacturing chains have visible depth — player/observer can trace raw ore → intermediate → component → module → ship
- Station construction creates expansion decisions — where to build, what type, what loadout

## Scope Boundaries

- **Not in scope:** Combat/weapons, multiplayer, NPC factions, market dynamics
- **Not in scope:** Ship visual models, 3D rendering, detailed ship physics (thrust vectoring, etc.)
- **Not in scope:** Construction ships (Phase 2 — for now, stations are deployed from kits)
- **Not in scope:** Moon bases (future — uses same frame+slot system but with terrain/gravity considerations)
- **Not in scope:** Population/workers (separate brainstorm — but hull/station slots are designed to accommodate future crew requirements per module)
- **Not in scope:** Leaders/bonuses (separate brainstorm — but hull bonuses and frame bonuses use the same StatModifier system that leaders will use later)

## Key Decisions

- **Unified hull+slot model for ships and stations:** Same architectural pattern, different content. Reduces implementation cost, creates consistent mental model, makes the StatModifier system (VIO-332) the universal modifier path.
- **Templates as the unit of construction:** Separates design (strategic) from execution (operational). Autopilot builds from templates, doesn't invent designs. Human gameplay = designing better templates.
- **Manufacturing depth varies by product:** YAGNI — don't make repair kits require 5 steps just for consistency. Depth is earned by product importance.
- **Content-driven everything:** New hulls, modules, recipes, and intermediates are JSON additions. Code only changes for new *mechanics* (new slot types, new physics interactions), not new *content*.
- **Phase 1 keeps current autopilot model:** Autopilot gets default templates, builds from them. Template optimization (learning/sim_bench) is a future enhancement.

## Phasing

### Phase 1: Foundation (builds on existing systems)
- Hull class types + slot system (ships)
- Station frame types + slot system
- 3-5 ship hull classes with recipes
- 3-4 station frame types with recipes
- 5-10 ship modules (propulsion, mining laser, cargo expander, scanner, etc.)
- 5-8 intermediate products (alloy plates, circuits, structural beams, etc.)
- Template system (create, validate, cost)
- Autopilot uses default templates
- Migrate current mining shuttle to General Purpose hull
- Migrate current station to Industrial Hub frame

### Phase 2: Depth
- Station construction (deploy station kits at new locations)
- Construction vessel hull class
- Station frame upgrades (add slots, increase capacity)
- Alternative recipes (basic vs advanced)
- More hull classes, modules, intermediates
- Autopilot template selection based on fleet needs

### Phase 3: Intelligence
- Autopilot template optimization (sim_bench learning)
- Blueprint sharing/import (future multiplayer prep)
- Manufacturing bottleneck detection and auto-rebalancing
- Byproduct chain optimization

## Dependencies / Assumptions

- **VIO-332 (StatModifier system)** is a hard prerequisite — hull bonuses, module stats, and frame bonuses all flow through it
- **VIO-322 (data-driven AnomalyTag)** pattern should be followed — hull classes, slot types, and module categories should be content-driven strings/newtypes, not compile-time enums where possible
- **Existing recipe system (Processor/Assembler)** is sufficient for all manufacturing — no new module behavior types needed, just more recipes
- **Current autopilot architecture** can select templates without major refactor — it already selects tasks by priority

## Outstanding Questions

### Resolve Before Planning

(All resolved)

### Deferred to Planning
- [Affects R2][Technical] Slot type implementation: fixed enum vs data-driven strings. Defer to planning — doesn't affect game design, only code structure.
- [Affects R9][Resolved] Station growth uses **both** tier upgrades (Mk1→Mk2→Mk3 via research+materials, adds slots/stats) **and** expansion modules (bolt-on modules that add capacity incrementally, cost a slot themselves). Tiers for major milestones, expansions for fine-tuning.
- [Affects R5][Technical] How to compute ship speed from mass + propulsion modules — extend existing `ticks_per_au` or new formula?
- [Affects R14][Needs research] What intermediate products create the most interesting production decisions? Needs content design pass.
- [Affects R11][Technical] Station kit deployment mechanics — new command type? New task type? How does it interact with the spatial system?
- [Affects R21][Technical] How does autopilot evaluate "fleet needs" to pick templates? Heuristic-based or metric-driven?

## Next Steps

→ `/ce:plan` for Phase 1 implementation planning.
