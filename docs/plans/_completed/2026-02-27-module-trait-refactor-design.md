# Station Module Trait Refactor — Design

**Ticket:** VIO-172
**Date:** 2026-02-27
**Status:** Draft

## Problem

All 5 station module tick functions repeat ~45 lines of boilerplate per module (extraction, interval check, power check, timer reset, wear application). Inconsistencies have crept in: wear efficiency applied in some modules but not others, timer reset placement varies, stall naming differs. Convention-based consistency is fragile — new modules will copy-paste and diverge further.

## Goal

Replace copy-pasted boilerplate with a shared module runner framework that enforces correct lifecycle behavior through structure, not convention. A new module should only need to implement its unique logic; interval, power, timer, and wear are handled by the framework.

## Approach: Extract-Then-Run

Separate the shared lifecycle (extract context, check interval/power, reset timer, apply wear) from module-specific logic (what happens during a "run"). Each module's `execute()` function receives a pre-built context and returns an outcome; the framework handles everything else.

### Why not traits?

Rust's borrow checker makes it painful to pass `&mut GameState` into trait methods that also need station access. The extract-then-run pattern avoids this by reading state into a context struct before the run, then writing results back after. More functional, more idiomatic Rust.

## Design

### 1. `ModuleTickContext<'a>` — shared extraction

```rust
pub(crate) struct ModuleTickContext<'a> {
    pub station_id: &'a StationId,
    pub module_idx: usize,
    pub module_id: &'a ModuleInstanceId,
    pub def: &'a ModuleDef,
    pub interval: u64,
    pub power_needed: f32,
    pub wear_per_run: f32,
    pub efficiency: f32, // wear_efficiency(wear_value, constants)
}
```

`extract_context()` does all the shared pre-checks:
- Returns `None` if module is disabled, power-stalled, def not found, or behavior is passive (Storage/Solar/Battery).
- Extracts the interval from the behavior-specific field name into a unified `interval`.
- Computes `efficiency` from current wear.
- Borrows `def`, `module_id`, `station_id` — no clones.

### 2. Interval semantics (defined precisely)

- **Timer increment:** Every tick, if enabled and not power-stalled.
- **Run condition:** `ticks_since_last_run >= interval`.
- **On Completed:** Reset to 0.
- **On Stalled/Skipped:** Module-specific (see below). The framework only resets on `Completed`; modules that want to reset on skip do so before returning.

`should_run()` encapsulates the interval check + power check. It increments the timer and returns whether the module should execute this tick.

### 3. `RunOutcome` — enforced post-run behavior

```rust
pub(crate) enum StallReason {
    VolumeCap { shortfall_m3: f32 },
    StockCap,
    DataStarved,
}

pub(crate) enum RunOutcome {
    /// Module ran successfully — framework resets timer, applies wear.
    Completed,
    /// Module can't run (no inputs, no target) — no wear.
    /// `reset_timer`: true = reset to 0 (e.g., lab no-tech-assigned),
    /// false = keep accumulating (e.g., processor below ore threshold).
    /// Module specifies intent; framework executes.
    Skipped { reset_timer: bool },
    /// Module is stalled (capacity/stock/data) — no wear, reset timer,
    /// manage stall flag + events.
    Stalled(StallReason),
}
```

`apply_run_result()` handles:
- **Completed:** Reset timer to 0, apply wear via `apply_wear()`, invalidate volume cache, clear stall flag if was stalled (emit resume event).
- **Skipped { reset_timer }:** No wear. Reset timer only if `reset_timer == true`. No stall flag changes.
- **Stalled:** No wear, reset timer to 0. Set stall flag, emit `ModuleStalled`/`AssemblerCapped`/`LabStarved` on transition (not-stalled -> stalled).

Events passed via `&mut Vec<EventEnvelope>` into `execute()` — no per-run Vec allocation.

### 4. Module `execute()` signatures

Each module exports an `execute()` function:

```rust
// processor.rs
pub(super) fn execute(
    ctx: &ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> RunOutcome;

// assembler.rs — also needs rng
pub(super) fn execute(
    ctx: &ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) -> RunOutcome;

// sensor.rs, lab.rs, maintenance.rs — similar
```

The `execute()` function contains ONLY the module-specific logic:
- Processor: ore threshold check, capacity pre-check, FIFO consume, recipe processing.
- Assembler: input availability, stock cap check, tech gate, recipe execution.
- Sensor: generate data.
- Lab: assigned tech check, data consumption, points production.
- Maintenance: find worn target, consume kit, apply repair.

### 5. Stall/resume unification

Volume-stall pattern (processor + assembler) is handled by `StallReason::VolumeCap`. The `execute()` functions return `RunOutcome::Stalled(VolumeCap { shortfall_m3 })` and the framework manages the `stalled` flag + events.

Lab starvation: `StallReason::DataStarved`. Same pattern.

Assembler capping: `StallReason::StockCap`. Same pattern.

The stall flag field names on `ModuleKindState` variants remain as-is for serialization compatibility (`stalled`, `capped`, `starved`). The framework knows which field to set based on the `StallReason` variant + module type.

Power stalling stays separate — handled by `compute_power_budget()` before module ticks. No change.

### 6. Tick ordering preserved

Current order: processors -> assemblers -> sensors -> labs -> maintenance (per station).

The framework runs 5 passes, one per module type, each using the shared extract/run/apply pattern. Zero behavior change. Can consolidate to single pass in a follow-up if desired.

```rust
pub(crate) fn tick_stations(state, content, rng, events) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        compute_power_budget(state, station_id, content, events);
        tick_modules_of_type::<Processor>(state, station_id, content, events);
        tick_modules_of_type::<Assembler>(state, station_id, content, rng, events);
        tick_modules_of_type::<Sensor>(state, station_id, content, events);
        tick_modules_of_type::<Lab>(state, station_id, content, events);
        tick_modules_of_type::<Maintenance>(state, station_id, content, events);
    }
}
```

In practice, since each module type needs a slightly different `execute()` signature (assembler needs `rng`), these will be 5 thin wrapper functions that call the shared `run_module_tick()` helper internally.

### 7. Wear efficiency — opt-in but visible

`ctx.efficiency` is always computed and available. Modules that produce scaled output use it. Modules that don't (maintenance repairs, discrete component production) ignore it deliberately.

Current behavior preserved:
- Processor: applies efficiency to material yield (keeps).
- Lab: applies efficiency to research points (keeps).
- Sensor: does NOT apply efficiency to data generation (keeps, but worth revisiting).
- Assembler: does NOT apply efficiency (discrete output, keeps).
- Maintenance: does NOT apply efficiency (repair is fixed, keeps).

### 8. Files changed

- `crates/sim_core/src/station/mod.rs` — Add `ModuleTickContext`, `RunOutcome`, `StallReason`, `extract_context()`, `apply_run_result()`, `run_module_tick()`. Modify `tick_stations()`.
- `crates/sim_core/src/station/processor.rs` — Replace `tick_station_modules()` with `execute()`.
- `crates/sim_core/src/station/assembler.rs` — Replace `tick_assembler_modules()` with `execute()`.
- `crates/sim_core/src/station/sensor.rs` — Replace `tick_sensor_array_modules()` with `execute()`.
- `crates/sim_core/src/station/lab.rs` — Replace `tick_lab_modules()` with `execute()`.
- `crates/sim_core/src/station/maintenance.rs` — Replace `tick_maintenance_modules()` with `execute()`.

### 9. What does NOT change

- `ModuleKindState` enum and per-module state structs (serialization compatibility).
- `ModuleBehaviorDef` enum and per-module def structs.
- `apply_wear()` function (already shared, stays as-is).
- `compute_power_budget()` (power system untouched).
- Event types and shapes.
- Tick ordering.
- All existing test behavior (tests are the correctness oracle).

### 10. Testing strategy

- All existing tests must pass unchanged (behavior preservation).
- Add a test for `extract_context()` returning `None` on disabled/power-stalled modules.
- Add a test for `apply_run_result()` handling each `RunOutcome` variant.
- Add a test verifying stall transition events (stall -> resume -> stall cycle).

## Risks

- **Borrow checker friction:** The context borrows from state, so `execute()` can't hold the context reference while mutating state. Mitigation: context fields are all Copy or borrowed, so we can destructure or clone cheaply where needed.
- **Serialization compatibility:** No type changes, so saved games load fine.
- **Behavior drift:** The refactor must be 100% behavior-preserving. Run `sim_bench` with deterministic seeds before and after to verify identical output.
