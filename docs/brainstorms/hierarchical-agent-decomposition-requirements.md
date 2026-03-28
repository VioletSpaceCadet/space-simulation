---
date: 2026-03-28
topic: hierarchical-agent-decomposition
---

# Hierarchical Agent Decomposition

## Problem Frame

The autopilot is a flat list of 10 peer behaviors (`StationModuleManager`, `LabAssignment`, `CrewAssignment`, `CrewRecruitment`, `ThrusterImport`, `SlagJettison`, `MaterialExport`, `PropellantPipeline`, `ShipFitting`, `ShipTaskScheduler`) that all see the full `GameState` and emit commands independently. This creates three problems:

1. **No scope isolation.** Every behavior reasons about the entire game state. Adding a new behavior requires understanding all others to avoid conflicts. Behaviors implicitly depend on execution order.
2. **No coordination.** There's no concept of "this station needs iron" that drives ship assignment. Ship task scheduling uses hardcoded priority (Deposit > Mine > DeepScan > Survey) with no awareness of station-level needs.
3. **Can't scale to multi-station.** With one station, global state ≈ station state. With multiple stations, behaviors need to know *which* station they're serving, which ships belong where, and how resources flow between stations. The flat model can't express this.

The current system also blocks the AI progression roadmap (see `docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md`). The `AutopilotConfig` schema (Phase 1 crux) needs to know what *scope* each config knob operates at — fleet-wide priority weights vs. station-level resource targets vs. ship-level behavior thresholds.

## Requirements

### Core Architecture

- R1. **Layer-agnostic agent trait.** A single trait that all decision-making agents implement, regardless of what tier they operate at. An agent receives objectives from its parent, reads scoped state, and emits either objectives for children or commands (at the leaf layer). The system should not hardcode a specific number of layers.
- R2. **Objective passing between layers.** Layers communicate via typed objective values, not by reaching into each other's state. Each layer trusts the layer below to handle execution details. A ship objective like "mine iron at asteroid X" doesn't prescribe the transit path, refueling stops, or docking procedure — the ship agent figures that out.
- R3. **Full awareness, scoped authority.** Agents see all state relevant to their scope level — a ship agent sees all asteroids, stations, and fuel options, not just its assigned target. But agents only *act* within their assigned objective scope. A ship told to "mine iron" can pick the best iron asteroid and decide to refuel first, but won't spontaneously switch to hauling water. Broad awareness enables adaptive behavior (rerouting when an asteroid depletes, opportunistic refueling); scoped authority prevents layers from stepping on each other.

### Ship Layer (Tactical)

- R4. **Ship agents handle tactical autonomy.** Given an objective (mine asteroid X, haul cargo to station Y, explore sector Z), a ship agent plans and executes the multi-step task sequence: transit, refuel if needed, execute task, return and deposit. The current `ShipTaskScheduler` logic moves here, but driven by objectives rather than a hardcoded priority chain.
- R5. **Ships have capabilities and aptitudes.** Hull type, fitted modules, and crew determine what tasks a ship *can* do and how effectively. A ship without mining equipment can't mine. A ship with mediocre mining capability may still be assigned mining when the priority is high enough and no better option exists.

### Station Layer (Operational)

- R6. **One agent per station.** Each station has its own agent instance that tracks its local goals (what resources it needs, what it wants to build, which modules to manage). Station A might prioritize iron production while Station B focuses on water extraction.
- R7. **Capability-aware ship assignment.** The station agent knows the capabilities and current state of its assigned ships and allocates tasks by matching priority against ship aptitude. It emits ship-level objectives, not micro-commands.
- R8. **Station-scoped behaviors consolidated.** Current behaviors that are inherently station-scoped (`StationModuleManager`, `LabAssignment`, `CrewAssignment`, `CrewRecruitment`, `ThrusterImport`, `SlagJettison`, `MaterialExport`, `PropellantPipeline`, `ShipFitting`) become sub-concerns of the station agent rather than independent global behaviors.

### Strategic Layer (Fleet-wide)

- R9. **Hybrid strategy source.** Config-driven defaults (priority weights, thresholds, expansion targets) interpreted by rules that read current state to produce concrete station-level objectives. The config is the interface that a player, optimizer (sim_bench parameter sweep), or LLM advisor can write to — same objective types regardless of the decision source.
- R10. **Strategic objectives are directional, not prescriptive.** The strategic layer says "prioritize iron throughput" or "shift to fleet expansion mode," not "station A build a mining ship" or "ship 3 mine asteroid 42." Translation to concrete actions is the station layer's job.

### Extensibility

- R11. **Intermediate layers can be inserted.** The architecture supports adding layers like sector/region coordinators (grouping stations) or squadron/fleet coordinators (grouping ships on shared missions) without restructuring existing layers. A new layer implements the same trait, accepts objectives from above, emits objectives to the layer below.
- R12. **Layer composition is declarative.** The hierarchy of layers (what feeds into what) should be expressed as configuration or composition, not hardcoded control flow. Adding a sector layer between strategic and station should not require modifying the strategic or station agent code.

### Migration

- R13. **Incremental migration from flat system.** Existing behaviors are migrated one at a time into the appropriate layer. Ship behaviors first (most self-contained), then station, then strategic. Old flat behaviors coexist with new layered agents during migration — the system doesn't require a big-bang rewrite.

## Success Criteria

- A new ship behavior (e.g., "patrol" or "escort") can be added by implementing ship-level logic only, without touching station or strategic code
- A new station can be added to the game and gets its own agent instance automatically, making independent resource/build decisions
- The strategic layer's config can be swapped (hand-tuned → optimizer-tuned → LLM-generated) without changing any layer below
- An intermediate layer (e.g., sector coordinator) can be inserted between strategic and station without modifying either
- Current autopilot behavior is preserved during and after migration (sim_bench regression test)

## Scope Boundaries

- **Not in scope:** LLM advisor implementation (that's the existing AI knowledge system roadmap). This designs the *interface* the LLM will write to.
- **Not in scope:** ML model integration (scoring functions, bottleneck prediction). Those plug into individual agent layers as decision helpers, not architectural concerns.
- **Not in scope:** Player-facing UI for setting strategic goals. The config interface is designed for it, but UI is separate work.
- **Not in scope:** Inter-station trade/logistics routing algorithms. The architecture supports station-to-station objectives, but the pathfinding/optimization logic is separate.
- **In scope but deferred:** Squadron/fleet and sector/region layers. The trait and objective system must *support* them, but implementing them is future work.

## Key Decisions

- **Objective passing over blackboard or events:** Objectives flow downward through the hierarchy. This is simpler to reason about than a shared blackboard (implicit coupling) or event-driven (ordering complexity). Each layer has a clear input (objectives from parent) and output (objectives to children or commands).
- **One agent per station:** Stations are independent decision-making entities, not a shared loop. This naturally supports multi-station divergence (different priorities per station) and maps cleanly to the entity model.
- **Ships have aptitudes, stations allocate:** Ships don't self-select tasks. The station layer does capability-weighted assignment, balancing task priority against ship capability. A mediocre miner gets mining duty when the need is critical.
- **Config + rules for strategy:** Neither pure config (too static) nor pure rules (too opaque). Config declares intent and weights; rules translate that into concrete objectives given current state. The config is the stable interface for player/optimizer/LLM — rules can evolve independently.
- **Incremental migration:** No big-bang rewrite. Ship extraction first (most contained), station consolidation second, strategic layer third. Old and new coexist during transition.

## Relationship to AI Progression Roadmap

This decomposition reshapes the 6-phase roadmap (`docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md`) by adding a *spatial/scope* dimension to the existing *temporal* dimension (per-tick / periodic / offline):

| Roadmap Phase | Impact |
|---------------|--------|
| Phase 1 (Strategy Interface) | `AutopilotConfig` becomes the strategic layer's config. Schema design is scoped to fleet-wide concerns, not a monolithic bag of every tuning knob. |
| Phase 2 (Classical Optimization) | Parameter sweep operates on strategic config. sim_bench compares strategies by varying config, not behavior code. |
| Phase 3 (Content Depth) | New game systems (crew, recipes) add capabilities to ship/station agents without touching the layer infrastructure. |
| Phase 4 (Rust Inference) | ML scoring functions plug into the station layer (asteroid scoring for ship assignment) or ship layer (route optimization). Scoped, not global. |
| Phase 5 (Multi-station) | Station agents are already independent. Strategic layer gains inter-station objective types (transfer resources, coordinate supply chains). Sector layer can be inserted. |
| Phase 6 (LLM Integration) | LLM writes to strategic config (same interface as Phase 2 optimizer). Natural separation: LLM reasons at fleet level, deterministic agents execute at station/ship level. |

## Phasing

### Phase A: Layer Infrastructure + Ship Extraction
- Define the agent trait and objective types
- Build the layer composition/execution system
- Extract `ShipTaskScheduler` into a ship-level agent driven by objectives
- Ship agent handles transit, refuel, mine, deposit, survey autonomously
- Existing flat behaviors continue running for everything else

### Phase B: Station Agent Consolidation
- Create per-station agent instances
- Migrate `StationModuleManager`, `LabAssignment`, `CrewAssignment`, `CrewRecruitment`, `ThrusterImport`, `SlagJettison`, `MaterialExport`, `PropellantPipeline`, `ShipFitting` into station agent sub-concerns
- Station agent emits ship objectives instead of direct commands
- Capability-aware ship assignment

### Phase C: Strategic Layer
- Define strategic config schema (priority weights, expansion targets, thresholds)
- Build rule interpreter that translates config + state → station objectives
- Connect to existing MCP advisor as a config source
- sim_bench can override strategic config per scenario

### Phase D: Multi-station + Intermediate Layers
- Station agents work independently at different orbital bodies
- Inter-station objectives (transfer, supply chain)
- Optional sector/region layer for grouping stations
- Optional squadron/fleet layer for coordinated ship missions

## Dependencies / Assumptions

- Hull+slot system (in progress) defines ship capabilities that feed into aptitude-based assignment
- Crew system (planned) adds another capability dimension for ships and stations
- Multi-station gameplay (not yet planned) is the primary driver for Phases C-D
- The existing `AutopilotBehavior` trait and `CommandSource` trait continue to work during migration — the new system composes with them, not replaces them immediately
- Determinism is preserved: same strategic config + same seed = same outcome, regardless of whether config came from hand-tuning, optimizer, or LLM

### Adaptive Defaults

- R14. **Every layer operates on defaults + signal-driven adaptation.** Each agent has sensible default behavior (from config or parent objectives) but reads local signals and metrics to adapt within its authority scope. A ship told to mine iron picks the best asteroid and reroutes if it depletes. A station escalates mining priority when inventory drops critically low. The strategic layer adjusts fleet objectives based on bottleneck trends. The balance between "follow orders" and "react to reality" is a per-layer design parameter, not a global switch.

## Outstanding Questions

### Deferred to Planning
- [Affects R1][Technical] Should the agent trait be async-capable for future LLM integration, or purely synchronous with async handled at the integration boundary?
- [Affects R2][Technical] Objective type design — should objectives be a single enum, per-layer enums, or trait objects? Tradeoffs: single enum is simple but couples layers; per-layer enums are decoupled but require translation; trait objects are most flexible but lose exhaustiveness checking.
- [Affects R7][Needs research] How should ship-to-station assignment work in multi-station? Are ships permanently assigned to a station, or can the strategic layer reassign them? What about ships in transit between stations?
- [Affects R12][Technical] How to express layer composition — tree structure in config, or Rust type-level composition (generics/associated types)?
- [Affects R13][Technical] During migration, how do flat behaviors and layered agents avoid conflicting commands for the same ship/station? Need a handoff protocol.

## Next Steps

→ Resolve the scoped state question above, then `/ce:plan` for Phase A (layer infrastructure + ship extraction).
