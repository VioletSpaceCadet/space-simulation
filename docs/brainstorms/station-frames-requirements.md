---
date: 2026-03-22
topic: station-frame-slot-system
parent: entity-depth-requirements.md
status: stub — expand in planning session
---

# Station Frame+Slot System

## Summary

Mirrors the ship hull+slot architecture for stations. Station frames define archetypes (Outpost, Industrial Hub, Research Station) with typed module slots, frame bonuses, and base stats. Existing station modules fit into typed slots. The shared SlotType newtype and ModifierSet pipeline (established in the ship hull project) are reused directly.

## Key Design Decisions (from entity-depth-requirements.md)

- **Unified slot architecture**: Same SlotType newtype, same ModuleDef.compatible_slots field. Station modules declare which slot types they fit.
- **Frame bonuses**: Static modifiers on the frame def, applied to station's ModifierSet. Same pattern as hull bonuses.
- **Station frame upgrades (R9)**: Both tier upgrades (Mk1→Mk2→Mk3 via research+materials, adds slots/stats) and expansion modules (bolt-on modules that add capacity incrementally, cost a slot themselves). Tiers for major milestones, expansions for fine-tuning.
- **Station construction from kits (R11)**: Phase 1 = instant deployment from kit (manufactured item). Phase 2 = construction ship required.
- **Station templates (R10)**: Covered by the separate templates sub-project, not this one.

## Scope

- FrameId newtype, FrameDef struct, frame_defs.json
- Existing modules get compatible_slots populated (processors → industrial, labs → research, etc.)
- StationState gets frame_id field, migration of current station to Industrial Hub
- 3-4 frame types with distinct profiles
- Frame upgrade path (tier system)
- Station construction command (DeployStationKit)
- Autopilot station management with frame awareness

## Dependencies

- Ship Hull+Slot System (establishes SlotType, ModuleDefId, Equipment variant, ModifierSource patterns)
- Manufacturing DAG System (station kits are manufactured items)

## Open Questions

- How does station deployment interact with the spatial system? New command type + new task type for a construction ship?
- Existing station has modules installed without slot constraints. Migration: assign all installed modules to slots, or grandfather them as "legacy" fittings?
- Frame upgrade recipes — manufactured upgrade kits or research-gated instant upgrades?
