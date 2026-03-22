---
date: 2026-03-22
topic: template-blueprint-system
parent: entity-depth-requirements.md
status: stub — expand in planning session
---

# Template & Blueprint System

## Summary

Templates are first-class game objects defining a hull/frame + module loadout. They are the unit of construction — shipyards and station constructors build from templates. Templates separate strategic decisions (what to build) from operational execution (building it). Both autopilot and future human players create/modify templates.

## Key Design Decisions (from entity-depth-requirements.md)

- **Templates as first-class objects (R19)**: Serializable, storable, shareable (future multiplayer). A template = hull/frame ID + ordered list of (slot_index, module_def_id) pairs.
- **Template validation (R20)**: At creation time — verify all modules fit their slot types, power budget doesn't exceed hull capacity, mass is within hull limits.
- **Autopilot template selection (R21)**: Autopilot evaluates fleet needs (mining capacity, transport capacity, scan coverage) and selects appropriate templates to build. Default templates provided in content.
- **Template cost computation (R22)**: Total material/component cost derived from hull recipe + all module recipes. Used by autopilot for build prioritization and UI for cost display.

## Scope

- TemplateId newtype, TemplateDef struct
- Template storage on GameState (BTreeMap)
- Template validation (slot compatibility, power budget)
- Template cost computation (recursive recipe cost rollup)
- Autopilot template selection based on fleet composition analysis
- Default templates in content JSON
- CreateTemplate / DeleteTemplate commands
- Template display in UI

## Dependencies

- Ship Hull+Slot System (templates reference hull classes and ship modules)
- Station Frame+Slot System (templates reference frames and station modules)
- Manufacturing DAG System (template cost = sum of all recipe costs through the DAG)

## Open Questions

- Should templates be content-only (defined in JSON) or also runtime-creatable (player designs custom templates)?
- Autopilot fleet needs analysis: heuristic-based (ratio of miners to haulers) or metric-driven (actual mining throughput vs transport throughput)?
- Template versioning: if a module def changes, do existing templates auto-update or need manual revision?
- Phase 1 fitting_templates.json (from ship hull project) evolves into this system — migration path?
