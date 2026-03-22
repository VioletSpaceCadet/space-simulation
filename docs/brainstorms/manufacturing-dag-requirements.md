---
date: 2026-03-21
topic: manufacturing-dag-chaining
---

# Manufacturing DAG / Production Chaining System

## Problem Frame

Current manufacturing is shallow — most products are 1-2 steps from raw ore. Ship construction (Fe + thrusters → ship) is the deepest chain at ~3 tiers. There's no meaningful production planning because every station runs the same simple pipeline. Deeper, branching production chains create supply chain planning decisions, reward research-driven recipe optimization, and make resource scarcity meaningful through location-dependent intermediates.

## Requirements

### Production Chain Architecture

- R1. **Multi-input recipes** across all module types (Processor and Assembler). Station inventory serves as a shared buffer. Recipes declare all required inputs; module checks inventory for all inputs before running. This already works for Assembler (Fe + thrusters → ship); extend pattern to Processors.
- R2. **Priority queuing for shared resources.** When multiple modules compete for the same intermediate (e.g., two assemblers both need Fe plates), module priority determines who consumes first. Uses the existing power priority pattern — manufacturing priority configurable per module or per module type.
- R3. **Intermediate products** as first-class inventory items. Examples:
  - **Tier 1 (refined):** Fe plates, Si wafers, refined H2O, carbon fiber
  - **Tier 2 (components):** Circuit boards (Si wafers), structural beams (Fe plates), insulation (carbon fiber)
  - **Tier 3 (subsystems):** Navigation computer (circuits + software), hull plating (Fe plates + beams), life support unit (H2O + O2 + circuits)
  - **Tier 4 (final products):** Ship modules, station modules, hull frames
- R4. **Variable chain depth by product.** Simple products (repair kits) stay 2 tiers. Ship hull frames require 4-5 tiers. Depth is content-defined, not code-enforced. The engine doesn't know or care about tier numbers — it just evaluates recipes.
- R5. **Byproduct chains.** Processing produces byproducts that feed other chains:
  - Slag reprocessing → trace minerals (new processor recipe)
  - H2O electrolysis → O2 (already produces LOX) → life support supply (crew system tie-in)
  - Smelting waste heat → thermal input for other processes (existing thermal system)
- R6. **All chains defined entirely in content JSON.** No code changes for new intermediates, recipes, or chain topologies. New element in elements.json + new recipe in module_defs.json = new production step.

### Recipe Research & Progression

- R7. **Basic recipes available from start.** Crude methods that work but are inefficient. Example: raw Fe smelting at 60% yield, 1.0 wear per run.
- R8. **Advanced recipes unlocked via research.** Better yield, lower waste, fewer inputs, or faster processing. Example: advanced Fe refining at 85% yield, 0.5 wear per run. Tech unlock adds the recipe to the module's available recipe list.
- R9. **Alternative recipes for the same output.** Different input combinations produce the same product. Example: hull plating from Fe plates (basic, heavy) OR from carbon-titanium composite (advanced, lighter, requires rare materials). Creates interesting material sourcing decisions.
- R10. **Recipe selection per module instance.** Each installed module can be configured to use a specific recipe from its available list. Autopilot selects based on available inputs and configured priority.

### Resource Scarcity & Location

- R11. **Element availability varies by asteroid type and zone.** Already partially implemented (IronRich vs VolatileRich templates, zone resource classes). Manufacturing chains should leverage this — advanced products need elements from different zones, creating inter-station demand.
- R12. **New elements for deeper chains.** Phase 1 additions: Carbon (from carbonaceous asteroids), Titanium (rare, from specific templates). These enable new intermediates (carbon fiber, titanium alloy) that gate advanced manufacturing.
- R13. **Single-station production in Phase 1.** Each station is self-contained. All inputs must be in station inventory. Inter-station supply routes deferred to Phase 2.

### Human Needs Manufacturing (ties into crew system)

- R14. **Consumer goods** as an aggregate item representing food, medicine, entertainment. Produced via recipe (inputs TBD — could be H2O + organic compounds, or simplified to "imports only" in Phase 1).
- R15. **Life support consumables** tied to crew count. O2 from electrolysis (LOX), CO2 scrubbing (new module or recipe), water recycling. Creates a steady-state consumption loop that scales with population.
- R16. **Human needs manufacturing chains** phased with crew system. Phase 1: consumer goods imported via trade. Phase 2: on-station production of life support consumables. Phase 3: full closed-loop life support.

### DAG Visibility

- R17. **Production chain visualization in UI.** Show the full DAG from raw materials to final products. Which intermediates are being produced, where bottlenecks are, what's backing up. Critical for supply chain planning gameplay.
- R18. **Station production overview.** Per-station view of all active recipes, their input/output rates, inventory levels for each intermediate, and bottleneck indicators.

## Success Criteria

- A ship hull frame requires 4+ distinct manufacturing steps from raw ore
- Player/observer can trace the full production chain visually
- Research unlocks meaningfully improve manufacturing (not just +10% efficiency)
- Different asteroid zones create different manufacturing capabilities (location matters)
- Autopilot manages multi-tier production without manual intervention
- Adding a new intermediate product = JSON content only

## Scope Boundaries

- **Not in scope:** Explicit routing / conveyor belts / Factorio-style physical layout
- **Not in scope:** Inter-station trade routes (Phase 2)
- **Not in scope:** Market pricing for intermediates (future economy expansion)
- **Not in scope:** Quality tiers on intermediates (existing quality system on materials is sufficient)
- **Not in scope:** Module-to-module direct connections / ports (station inventory buffer is the routing mechanism)

## Key Decisions

- **Station inventory as shared buffer + priority queuing:** No explicit routing. Modules pull from and push to station inventory. Priority resolves contention. Simplest architecture that works; Factorio-style routing is overkill for this game's abstraction level.
- **Variable depth, content-defined:** Engine doesn't enforce tiers. Depth emerges from recipe dependencies in content JSON.
- **Alternative recipes create meaningful choice:** Not just "better recipe replaces old one" — different inputs, different trade-offs, both valid depending on available resources.
- **Human needs as manufacturing chains:** Consumer goods and life support are produced items, not magic. Ties crew system to manufacturing naturally.

## Phasing

### Phase 1: Chain Depth
- 4-6 new intermediate products (Fe plates, Si wafers, circuits, structural beams, carbon fiber, hull plating)
- Multi-input recipes on existing Processors and Assemblers
- Priority queuing for shared resources
- 2-3 new elements (Carbon, Titanium)
- Ship hull frames as 4-tier products
- Basic recipes for all chains

### Phase 2: Research & Alternatives
- Advanced recipe variants unlocked via tech tree
- Alternative recipes (different inputs → same output)
- Recipe selection per module instance
- Slag reprocessing chain
- Production chain visualization in UI

### Phase 3: Scarcity & Logistics
- Inter-station trade routes (transport ships move intermediates)
- Autopilot supply chain planning
- Rare element types gating advanced products
- Station specialization (mining outpost → refining hub → assembly complex)

### Phase 4: Human Needs
- Consumer goods production chain
- Life support manufacturing (O2, CO2 scrubbing, water recycling)
- Closed-loop life support as late-game research goal

## Dependencies / Assumptions

- **Entity depth system** should land first — manufacturing produces hull frames, ship modules, station modules
- **Existing Processor/Assembler architecture** handles all recipe types without new module behavior types
- **Crew system** creates demand for human needs manufacturing (Phase 4)
- **Research system** provides recipe unlock mechanism (tech effects → recipe availability)

## Outstanding Questions

### Resolve Before Planning

(None — all blocking questions resolved)

### Deferred to Planning
- [Affects R3][Needs research] What specific intermediates create the most interesting production decisions? Needs content design pass.
- [Affects R2][Technical] How does manufacturing priority interact with existing power priority? Same system or separate?
- [Affects R10][Technical] Recipe selection UX — how does autopilot choose between alternative recipes? Cost minimization? Output quality?
- [Affects R14][Needs research] What inputs should consumer goods require? Pure import, or producible from existing elements?
- [Affects R17][Technical] What visualization best shows a production DAG in the existing panel system?

## Next Steps

→ `/ce:plan` for Phase 1 implementation (after entity depth lands).
