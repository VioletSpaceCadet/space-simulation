# Split station.rs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Split `crates/sim_core/src/station.rs` (2,619 lines) into a `station/` module directory with 7 files.

**Architecture:** Create `station/` directory, move each tick function + its helpers to a dedicated file, extract `check_power()` and `apply_wear()` to `helpers.rs`. Pure file reorg + one dedup extraction. No logic changes.

**Tech Stack:** Rust module system (`mod.rs` + submodules, `pub(super)` visibility)

---

### Task 1: Create station/mod.rs with orchestrator and constants

**Files:**
- Create: `crates/sim_core/src/station/mod.rs`
- Delete (later, Task 7): `crates/sim_core/src/station.rs`

**Step 1: Create directory**

Run: `mkdir -p crates/sim_core/src/station`

**Step 2: Write mod.rs**

Create `crates/sim_core/src/station/mod.rs` with:
- Module declarations for all submodules
- Constants `MIN_MEANINGFUL_KG` and `TECH_SHIP_CONSTRUCTION` as `pub(super)`
- The `tick_stations()` function (lines 76-98 of original)
- `use` statements for submodule functions

```rust
mod assembler;
mod helpers;
mod lab;
mod maintenance;
mod processor;
mod sensor;

use crate::{EventEnvelope, GameContent, GameState, StationId};

use assembler::tick_assembler_modules;
use lab::tick_lab_modules;
use maintenance::tick_maintenance_modules;
use processor::tick_station_modules;
use sensor::tick_sensor_array_modules;

/// Minimum meaningful mass — amounts below this are discarded as rounding noise.
pub(super) const MIN_MEANINGFUL_KG: f32 = 1e-3;

/// Tech ID required for ship construction recipes.
pub(super) const TECH_SHIP_CONSTRUCTION: &str = "tech_ship_construction";

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_assembler_modules(state, station_id, content, rng, events);
    }
    for station_id in &station_ids {
        tick_sensor_array_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_lab_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_maintenance_modules(state, station_id, content, events);
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p sim_core`
Expected: Compile error (submodules don't exist yet). That's fine — we'll create them next.

**Step 4: Commit**

Do NOT commit yet — wait until all files are created and tests pass.

---

### Task 2: Create station/helpers.rs

**Files:**
- Create: `crates/sim_core/src/station/helpers.rs`

**Step 1: Write helpers.rs**

Contains `check_power()` (new extraction) and `apply_wear()` (moved from lines 1623-1663).

```rust
use crate::{Event, EventEnvelope, GameState, StationId};

/// Returns true if the station has enough power for this module run.
pub(super) fn check_power(state: &GameState, station_id: &StationId, power_needed: f32) -> bool {
    state
        .stations
        .get(station_id)
        .map_or(false, |s| s.power_available_per_tick >= power_needed)
}

pub(super) fn apply_wear(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    wear_per_run: f32,
    events: &mut Vec<EventEnvelope>,
) {
    if wear_per_run <= 0.0 {
        return;
    }
    let current_tick = state.meta.tick;
    if let Some(station) = state.stations.get_mut(station_id) {
        let module = &mut station.modules[module_idx];
        let wear_before = module.wear.wear;
        module.wear.wear = (module.wear.wear + wear_per_run).min(1.0);
        let wear_after = module.wear.wear;

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::WearAccumulated {
                station_id: station_id.clone(),
                module_id: module.id.clone(),
                wear_before,
                wear_after,
            },
        ));
        if module.wear.wear >= 1.0 {
            let mid = module.id.clone();
            module.enabled = false;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleAutoDisabled {
                    station_id: station_id.clone(),
                    module_id: mid,
                },
            ));
        }
    }
}
```

---

### Task 3: Create station/processor.rs

**Files:**
- Create: `crates/sim_core/src/station/processor.rs`

**Step 1: Write processor.rs**

Move from original station.rs:
- `estimate_output_volume_m3()` (lines 19-60)
- `matches_input_filter()` (lines 62-74)
- `tick_station_modules()` (lines 101-300) — replace inline power check with `super::helpers::check_power()`
- `resolve_processor_run()` (lines 303-474) — replace `apply_wear()` call with `super::helpers::apply_wear()`
- `consume_ore_fifo_with_lots()` (lines 479-519)
- `peek_ore_fifo_with_lots()` (lines 1573-1600)
- `slag_composition_from_avg()` (lines 1603-1621)

Key changes:
- Add `use super::MIN_MEANINGFUL_KG;` and other needed imports
- Replace `apply_wear(` calls with `super::helpers::apply_wear(`
- Replace inline power check block (lines 155-163) with:
  ```rust
  if !super::helpers::check_power(state, station_id, power_needed) {
      continue;
  }
  ```
- Function visibility: `pub(super) fn tick_station_modules(...)`, all others private
- Keep `#[allow(clippy::too_many_lines)]` on `tick_station_modules` and `resolve_processor_run`
- Make `matches_input_filter` and `peek_ore_fifo_with_lots` `pub(super)` — they're tested in mod.rs tests

---

### Task 4: Create station/assembler.rs

**Files:**
- Create: `crates/sim_core/src/station/assembler.rs`

**Step 1: Write assembler.rs**

Move from original station.rs:
- `tick_assembler_modules()` (lines 522-889)
- `resolve_assembler_run()` (lines 892-1070)

Key changes:
- Add `use super::MIN_MEANINGFUL_KG;` and `use super::TECH_SHIP_CONSTRUCTION;`
- Replace inline power check with `super::helpers::check_power()`
- Replace `apply_wear()` with `super::helpers::apply_wear()`
- Function visibility: `pub(super) fn tick_assembler_modules(...)`, `resolve_assembler_run` stays private
- Keep `#[allow(clippy::too_many_lines)]` and `#[allow(clippy::too_many_arguments)]` attributes

---

### Task 5: Create station/sensor.rs, station/lab.rs, station/maintenance.rs

**Files:**
- Create: `crates/sim_core/src/station/sensor.rs`
- Create: `crates/sim_core/src/station/lab.rs`
- Create: `crates/sim_core/src/station/maintenance.rs`

**Step 1: Write sensor.rs**

Move `tick_sensor_array_modules()` (lines 1072-1162). Replace inline power check with helper. Replace `apply_wear()` with `super::helpers::apply_wear()`. Visibility: `pub(super)`.

**Step 2: Write lab.rs**

Move `tick_lab_modules()` (lines 1165-1380). Replace inline power check with helper. Replace `apply_wear()` with `super::helpers::apply_wear()`. Visibility: `pub(super)`. Keep `#[allow(clippy::too_many_lines)]`.

**Step 3: Write maintenance.rs**

Move `tick_maintenance_modules()` (lines 1383-1568). Replace inline power check with helper. Visibility: `pub(super)`. Keep `#[allow(clippy::too_many_lines)]`. Note: maintenance does NOT call `apply_wear()` — it has its own repair logic.

---

### Task 6: Move tests to mod.rs

**Files:**
- Modify: `crates/sim_core/src/station/mod.rs`

**Step 1: Add test modules**

Append the 3 test modules from the original file (lines 1665-2619) to the bottom of `mod.rs`:
- `mod tests` (ore FIFO tests) — needs `use super::*;` replaced with explicit imports since `peek_ore_fifo_with_lots` and `matches_input_filter` are in `processor.rs`. Use `use processor::{peek_ore_fifo_with_lots, matches_input_filter};` (requires those functions to be `pub(super)`)
- `mod lab_tests` — calls `super::tick_lab_modules` → change to `super::lab::tick_lab_modules` or use the re-import
- `mod assembler_component_tests` — calls `super::tick_assembler_modules` → change to `super::assembler::tick_assembler_modules`
- `mod sensor tests` (inside lab_tests) — calls `super::tick_sensor_array_modules` → change to `super::sensor::tick_sensor_array_modules`

For test `super::*` imports:
- `mod tests`: change `super::*` to `use super::processor::{peek_ore_fifo_with_lots};` plus needed crate types
- `mod lab_tests`: change `super::tick_lab_modules` to `super::lab::tick_lab_modules`, and `super::tick_sensor_array_modules` to `super::sensor::tick_sensor_array_modules`
- `mod assembler_component_tests`: change `super::tick_assembler_modules` to `super::assembler::tick_assembler_modules`

---

### Task 7: Delete original station.rs and verify

**Step 1: Delete the original file**

Run: `rm crates/sim_core/src/station.rs`

The Rust compiler will now use `station/mod.rs` instead.

**Step 2: Run all sim_core tests**

Run: `cargo test -p sim_core`
Expected: All tests pass.

**Step 3: Run clippy**

Run: `cargo clippy -p sim_core`
Expected: No new warnings.

**Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass across all crates.

**Step 5: Verify line counts**

Run: `wc -l crates/sim_core/src/station/*.rs`

Expected approximate:
- mod.rs: ~1000 lines (orchestrator + tests)
- helpers.rs: ~50 lines
- processor.rs: ~520 lines
- assembler.rs: ~560 lines
- sensor.rs: ~95 lines
- lab.rs: ~220 lines
- maintenance.rs: ~190 lines

**Step 6: Commit**

```bash
git add -A crates/sim_core/src/station/
git add crates/sim_core/src/station.rs  # stages the deletion
git commit -m "refactor(sim_core): split station.rs into module directory (VIO-49)

Split 2,619-line station.rs into station/ with 7 files:
- mod.rs: orchestrator, constants, tests
- helpers.rs: check_power(), apply_wear()
- processor.rs, assembler.rs, sensor.rs, lab.rs, maintenance.rs

Extracted check_power() helper (was duplicated 5x).
No logic changes — pure file reorganization.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```
