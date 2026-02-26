# Station Tick Optimization Design (VIO-82)

**Date:** 2026-02-25
**Ticket:** VIO-82
**Status:** Approved

## Problem

Station tick takes ~27µs per tick due to 5 separate passes over all modules, each doing O(n) linear scans on `module_defs` Vec. This caused a ~4x regression (1.2M → 300K TPS in release mode) when research system added 3 more passes.

## Changes

### 1. HashMap module_defs

Convert `GameContent.module_defs` from `Vec<ModuleDef>` to `HashMap<String, ModuleDef>`. 11 `.iter().find(|d| d.id == ...)` call sites become `.get(...)`. Test fixtures change from `vec![]` to `HashMap::from()`.

### 2. Single-pass station tick

Merge 5 station tick functions into 1 loop with match dispatch:

```rust
for module in &station.modules {
    let def = content.module_defs.get(&module.def_id);
    match &def.behavior {
        Processor(..) => tick_processor(..),
        Assembler(..) => tick_assembler(..),
        SensorArray(..) => tick_sensor(..),
        Lab(..) => tick_lab(..),
        MaintenanceBay(..) => tick_maintenance(..),
    }
}
```

Existing per-type functions become helpers called from the single loop.

## Out of Scope

- Numeric IDs (VIO-42)
- Incremental volume caching (VIO-83, already on main)
