---
date: 2026-03-21
topic: exploration-artifacts-discovery
---

# Exploration / Artifacts / Discovery System

## Problem Frame

The current exploration loop (scan → survey → deep scan) only discovers ore-bearing asteroids. There's nothing else to find. Adding discoverable anomalies, artifacts, and derelict structures creates exploration as a rewarding activity in its own right — not just a prerequisite for mining. Phase 1 adds the discovery primitives. The long-term vision is far larger: procedurally generated civilizational history, emergent lore, and multi-system exploration.

## Long-Term Vision (captured for context, not Phase 1 scope)

The simulation engine can run civilizations forward in time autonomously. The ultimate version of this system:

- **Procedural history generation:** Run multiple civilizations through the sim for thousands of ticks. Some thrive, some collapse. Record their history — what they built, where they expanded, what killed them.
- **Artifact emergence from history:** Collapsed civilizations leave behind artifacts, derelict stations, abandoned technology. These are real game objects generated from actual simulation runs, not hand-authored content.
- **Lore from simulation:** A civilization that collapsed due to thermal cascade leaves behind heat-damaged stations with advanced thermal tech. One that ran out of resources leaves stripped mining outposts. The artifacts tell a story because they came from a real simulation.
- **Living civilizations:** Other civilizations running in parallel as the player plays. First contact, trade, conflict, diplomacy — all emerging from simulated agents with their own autopilots.
- **Multi-solar-system:** Expansion beyond the home system. FTL or generation ships. Each system has its own history layer.
- **Races/species:** Different starting bonuses, research progressions, cultural traits. Affects how civilizations develop and what artifacts they leave behind.

This is a 10+ project roadmap. Phase 1 below is the foundation that doesn't depend on any of it but is designed to grow into it.

## Requirements (Phase 1: Discovery Primitives)

### Anomaly System

- R1. **Anomaly sites** as a new discoverable entity type alongside asteroids. Generated during scan site replenishment with configurable probability. Anomalies appear in the scan site pool but require investigation (not just survey).
- R2. **Anomaly types** defined in content JSON: `anomaly_templates.json`. Each template has: id, name, description, rarity, investigation_ticks, required_tech (optional gate), and reward definitions.
- R3. **Investigation task** for ships. New TaskKind: `Investigate { anomaly_id, ticks_remaining }`. Ship travels to anomaly site, spends investigation_ticks investigating, then resolves the anomaly's reward. Requires specific ship capability (scanner module or research module in ship fit).
- R4. **Anomaly rewards** use the same effect system as random events (R7 from events doc): AddInventory, AddResearchData, SpawnScanSite, ApplyModifier, plus new effects:
  - `UnlockBlueprint { blueprint_id }` — grants a unique recipe/module blueprint not available through normal research
  - `RevealZone { zone_id }` — reveals scan sites in a previously unknown zone (multi-system prep)
  - `AddArtifact { artifact_def_id }` — adds a unique artifact item to station inventory

### Artifact Items

- R5. **Artifacts as inventory items** with a special `Artifact` kind. Each artifact has unique properties defined in content. Artifacts can be: stored, transported (physical transit), studied (generates research data over time), or installed (provides permanent stat modifier).
- R6. **Artifact study** mechanic. Assign an artifact to a research lab. The lab generates bonus research data in a specific domain while studying the artifact. Study completes after N ticks, potentially unlocking the artifact's special properties or a unique tech.
- R7. **Artifact installation** for permanent benefits. Some artifacts can be installed in a station or ship module slot, providing a unique StatModifier. Example: "Ancient Navigation Core" installed in a ship provides -20% transit time. Limited by slot type and hull/frame compatibility.

### Initial Content

- R8. **Phase 1 anomaly types (~5-8):**
  - Derelict probe — investigation yields research data burst (Survey + Manufacturing)
  - Mineral anomaly — reveals a hidden asteroid with rare composition (high Ti or rare elements)
  - Energy signature — investigation yields blueprint for improved solar array or battery
  - Abandoned cache — contains components or materials (random from loot table)
  - Strange signal — requires advanced scanner; yields large research data burst + artifact chance
  - Ice formation — reveals volatile-rich asteroid cluster in unusual location
- R9. **Phase 1 artifacts (~3-5):**
  - Ancient Navigation Core — ship module slot, -15% transit time
  - Alien Alloy Sample — research study generates Materials domain data; after study, unlocks "alien alloy" recipe
  - Efficient Reactor Fragment — station installation, +10% power output
  - Data Crystal — instant large research data injection across all domains

### Discovery Progression

- R10. **Anomaly rarity scales with zone distance.** Close zones (Earth orbit) have common anomalies. Distant zones (Jupiter Trojans, outer belt) have rare/legendary anomalies. Creates exploration incentive to expand outward.
- R11. **Investigation tech gates.** Basic anomalies investigable with any ship. Advanced anomalies (strange signal, energy signature) require specific tech unlocks or ship modules. Research drives exploration capability.
- R12. **Anomaly replenishment** follows scan site pattern — configurable rate, target count per zone, weighted by zone properties.

## Success Criteria

- Exploration has rewards beyond ore (artifacts, blueprints, research boosts)
- Different zones have different discovery potential (reason to explore far)
- Artifacts create unique strategic advantages not available through normal manufacturing
- Adding a new anomaly type = JSON content only
- Discovery events appear in the event feed, creating narrative moments
- Foundation is extensible toward the long-term vision without architectural rework

## Scope Boundaries

- **Not in scope:** Procedural history generation (long-term vision, separate project)
- **Not in scope:** Living NPC civilizations (long-term vision)
- **Not in scope:** Multi-solar-system (long-term vision, requires FTL/generation ship systems)
- **Not in scope:** Races/species (long-term vision)
- **Not in scope:** Artifact crafting/combining (future enhancement)
- **Not in scope:** Anomaly combat encounters (requires warfare system)
- **Design for future:** Anomaly template format should accommodate future "derelict station" anomalies that are full explorable entities. Artifact format should accommodate future "civilization origin" metadata.

## Key Decisions

- **Anomalies use the same scan site discovery pipeline.** No new discovery mechanic — anomalies appear in the scan site pool during replenishment. This reuses existing infrastructure.
- **Rewards use the event effect system.** Anomaly resolution applies effects from the same generic effect system as random events. One effect system for everything.
- **Artifacts are inventory items with special properties.** Not a separate system — they're items that can be stored, transported, studied, installed. Fits into existing inventory model.
- **Phase 1 is hand-authored content.** Anomaly templates and artifact defs are manually created JSON. Procedural generation from simulated history is the long-term vision, not Phase 1.

## Phasing

### Phase 1: Discovery Primitives
- Anomaly site entity type + anomaly_templates.json
- Investigation ship task
- 5-8 anomaly types with rewards
- 3-5 artifact items with study/install mechanics
- Anomaly rarity by zone distance
- Tech-gated investigation
- SSE events for discovery moments

### Phase 2: Depth
- More anomaly types (15-20)
- Multi-step investigation chains (first investigation reveals location of deeper anomaly)
- Artifact study yielding unique tech unlocks
- Derelict station anomalies (explorable, salvageable)
- Anomaly map/log in UI

### Phase 3: Procedural History (The Big One)
- Civilization simulation framework (run autopilot civs forward in time)
- Collapse mechanics (resource depletion, thermal cascade, crew crisis → civ death)
- Decay system (abandoned structures degrade over time)
- Artifact generation from simulation history (real artifacts from real simulated civs)
- Lore generation from simulation events (what happened to this civilization)

### Phase 4: Living Universe
- NPC civilizations running in parallel
- First contact mechanics
- Trade with NPC civs
- Multi-solar-system expansion
- Species/race system with starting bonuses

## Dependencies / Assumptions

- **Entity depth system** — artifacts install into ship/station module slots
- **Event effect system** — anomaly rewards use generic effects
- **StatModifier system** (VIO-332) — artifact installation bonuses
- **Research system** — artifact study generates domain data, tech gates investigation
- **Crew system** (optional) — investigation may require scientist crew on ship

## Outstanding Questions

### Resolve Before Planning

(None — all blocking questions resolved)

### Deferred to Planning
- [Affects R1][Technical] How do anomaly sites integrate with the scan site spatial model? Same ScanSite with an anomaly flag, or separate entity type?
- [Affects R3][Technical] Investigation task — new TaskKind variant or reuse existing Survey/DeepScan with anomaly target?
- [Affects R6][Technical] Artifact study — new lab mode, or artifact acts as a "data source" plugged into existing lab data consumption?
- [Affects R10][Needs research] What anomaly density per zone feels right? Needs playtesting.
- [Affects R8][Needs research] What specific rewards make anomalies feel exciting without being overpowered?

## Next Steps

→ `/ce:plan` for Phase 1 implementation (after entity depth and event system land).
