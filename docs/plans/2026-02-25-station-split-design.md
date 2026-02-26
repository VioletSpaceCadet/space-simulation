# Split station.rs into module directory

**Linear:** VIO-49
**Date:** 2026-02-25

## Problem

`crates/sim_core/src/station.rs` is 2,619 lines with 5 independent module tick systems in one file.

## Design

Split into `station/` module directory with flat submodules:

```
station/
  mod.rs           tick_stations() orchestrator, constants, tests
  helpers.rs       check_power(), apply_wear()
  processor.rs     tick_station_modules(), resolve_processor_run(), ore FIFO helpers, slag helpers
  assembler.rs     tick_assembler_modules(), resolve_assembler_run()
  sensor.rs        tick_sensor_array_modules()
  lab.rs           tick_lab_modules()
  maintenance.rs   tick_maintenance_modules()
```

### Shared helpers

- **`check_power(state, station_id, power_needed) -> bool`**: Extracts 5 identical 7-line blocks into one function.
- **`apply_wear()`**: Already a standalone function, moves to helpers.rs.
- **Timer blocks**: Stay inline in each module file (8 lines each, variant-specific destructuring makes extraction awkward).
- **Capacity/stall logic**: Stays in processor.rs and assembler.rs respectively (different enough that extraction adds complexity).

### Visibility

- `tick_stations()` remains `pub(crate)` — only public API
- Submodule functions are `pub(super)` — visible only within `station/`
- Helper functions are `pub(super)`
- Constants (`MIN_MEANINGFUL_KG`, `TECH_SHIP_CONSTRUCTION`) in `mod.rs` as `pub(super)`

### Tests

Existing `mod tests` stays in `mod.rs` with `use super::*` pulling in re-exports from submodules.

### What doesn't change

- No logic changes — pure file reorganization + `check_power()` extraction
- `engine.rs` import unchanged: `use crate::station::tick_stations;`
- All existing tests pass unchanged
