---
title: "Adding a new module behavior type: cross-layer checklist"
category: integration-issues
date: 2026-02-28
module: sim_core, sim_world, sim_control, ui_web
component: types.rs, station/mod.rs, applyEvents.ts
tags: [module-system, extensibility, checklist, cross-layer, radiator]
project: Heat System
tickets: [VIO-207, VIO-208]
---

## Problem

Adding a new module behavior type (like Radiator) requires touching many files across all layers of the codebase. Missing any one location causes compile errors, runtime panics, or silent FE bugs. The Radiator addition (VIO-207) touched 13+ files and required coordinating changes across Rust, TypeScript, and JSON content.

## Root Cause

The module system uses enum variants in both `ModuleBehaviorDef` (content definition) and `ModuleKindState` (runtime state). Rust's exhaustive match ensures compile-time coverage within a single crate, but the cross-crate and cross-language touchpoints are easy to miss.

## Solution: Checklist for adding a new module behavior type

### sim_core (types.rs)
1. Add variant to `ModuleBehaviorDef` enum (e.g., `Radiator(RadiatorDef)`)
2. Add variant to `ModuleKindState` enum (e.g., `Radiator(RadiatorState)`)
3. Define the `*Def` and `*State` structs with Serialize/Deserialize

### sim_core (station logic)
4. Add match arm in `power_priority()` (power consumption for the module type)
5. Add match arm in `extract_context()` (context extraction for tick processing)
6. Add tick logic (either in existing tick step or new step like `tick_thermal`)
7. Update `apply_run_result()` if the module produces wear or events

### sim_core (engine.rs)
8. Add match arm in `InstallModule` command handler for the new behavior

### sim_control (autopilot)
9. Update `AutopilotController` if autopilot should manage this module type

### sim_world (content)
10. Add module definition to `content/module_defs.json`
11. Add content validation tests in `sim_world`

### sim_bench (overrides)
12. Add override support for the new module type's tunable fields

### ui_web (TypeScript)
13. Add variant to `ModuleKindState` union type in `types.ts`
14. Add entry to `MODULE_KIND_STATE_MAP` in `applyEvents.ts`
15. Add Zod schema for any new events
16. Update panel rendering if the module has visible state

### CI
17. Run `./scripts/ci_event_sync.sh` to verify any new events are handled

### Content
18. Add module to `dev_base_state.json` if it should be available from game start
19. Add test fixtures (`state_with_*()` helpers) for the new module type

## Prevention

- Before starting a new module type, copy this checklist into your implementation plan.
- Rust compiler catches most sim_core touchpoints via exhaustive match.
- `ci_event_sync.sh` catches missing FE event handlers.
- The gap is TypeScript types and `MODULE_KIND_STATE_MAP` — these are not auto-checked. Review manually.
