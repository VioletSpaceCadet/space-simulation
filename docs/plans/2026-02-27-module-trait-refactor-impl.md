# Station Module Trait Refactor — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace copy-pasted boilerplate across 5 station module tick functions with a shared extract-then-run framework that enforces correct lifecycle behavior.

**Architecture:** A `ModuleTickContext<'a>` struct extracts shared state once per module. Each module type implements an `execute()` function containing only its unique logic, returning a `RunOutcome`. A shared `apply_run_result()` handles timer reset, wear, stall transitions, and events. Five passes preserve current tick ordering.

**Tech Stack:** Rust (sim_core crate), no new dependencies.

**Design doc:** `docs/plans/2026-02-27-module-trait-refactor-design.md`

---

### Task 1: Add shared types and `extract_context()` to mod.rs

**Files:**
- Modify: `crates/sim_core/src/station/mod.rs`

**Step 1: Write the failing test for `extract_context()`**

Add at the bottom of `mod.rs`, inside a new `#[cfg(test)] mod framework_tests` block:

```rust
#[cfg(test)]
mod framework_tests {
    use super::*;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn test_content_with_processor() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_refinery".to_string(),
            ModuleDef {
                id: "module_refinery".to_string(),
                name: "Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }),
            },
        );
        content
    }

    fn test_state_with_module(content: &GameContent, kind_state: ModuleKindState) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState { tick: 10, seed: 42, schema_version: 1, content_version: content.content_version.clone() },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: NodeId("node_test".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("refinery_inst_0001".to_string()),
                        def_id: "module_refinery".to_string(),
                        enabled: true,
                        kind_state,
                        wear: WearState::default(),
                        power_stalled: false,
                    }],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                },
            )]),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
                action_counts: HashMap::new(),
            },
            balance: 0.0,
            counters: Counters::default(),
        }
    }

    #[test]
    fn extract_context_returns_some_for_enabled_processor() {
        let content = test_content_with_processor();
        let state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 3,
            stalled: false,
        }));
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content);
        assert!(ctx.is_some(), "should return context for enabled processor");
        let ctx = ctx.unwrap();
        assert_eq!(ctx.module_idx, 0);
        assert_eq!(ctx.interval, 5);
        assert!((ctx.power_needed - 10.0).abs() < 1e-3);
        assert!((ctx.wear_per_run - 0.01).abs() < 1e-3);
        assert!((ctx.efficiency - 1.0).abs() < 1e-3); // no wear = full efficiency
    }

    #[test]
    fn extract_context_returns_none_for_disabled_module() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
        }));
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;
        assert!(extract_context(&state, &station_id, 0, &content).is_none());
    }

    #[test]
    fn extract_context_returns_none_for_power_stalled() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
        }));
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().modules[0].power_stalled = true;
        assert!(extract_context(&state, &station_id, 0, &content).is_none());
    }

    #[test]
    fn extract_context_returns_none_for_storage() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(&content, ModuleKindState::Storage);
        let station_id = StationId("station_test".to_string());
        // Change def to storage
        let mut content2 = content.clone();
        content2.module_defs.insert("module_refinery".to_string(), ModuleDef {
            id: "module_refinery".to_string(),
            name: "Storage".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Storage { capacity_m3: 500.0 },
        });
        assert!(extract_context(&state, &station_id, 0, &content2).is_none());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core extract_context`
Expected: FAIL — `extract_context` not found.

**Step 3: Implement shared types and `extract_context()`**

Add above the `tick_stations()` function in `mod.rs`:

```rust
/// Context extracted once per module, shared across the lifecycle.
pub(crate) struct ModuleTickContext<'a> {
    pub station_id: &'a StationId,
    pub module_idx: usize,
    pub module_id: &'a ModuleInstanceId,
    pub def: &'a crate::ModuleDef,
    pub interval: u64,
    pub power_needed: f32,
    pub wear_per_run: f32,
    pub efficiency: f32,
}

/// Reason a module stalled (distinct from "skipped").
#[derive(Debug)]
pub(crate) enum StallReason {
    VolumeCap { shortfall_m3: f32 },
    StockCap,
    DataStarved,
}

/// Outcome of a module's execute() call.
#[derive(Debug)]
pub(crate) enum RunOutcome {
    /// Module ran successfully — framework resets timer, applies wear.
    Completed,
    /// Module can't run (no inputs, no target) — no wear.
    /// `reset_timer`: true = reset to 0, false = keep accumulating.
    /// Module specifies intent; framework executes.
    Skipped { reset_timer: bool },
    /// Module is stalled — framework resets timer, manages stall flag + events.
    Stalled(StallReason),
}

/// Extract shared module context. Returns None if the module should be skipped
/// entirely (disabled, power-stalled, passive type, missing def).
fn extract_context<'a>(
    state: &'a GameState,
    station_id: &'a StationId,
    module_idx: usize,
    content: &'a GameContent,
) -> Option<ModuleTickContext<'a>> {
    let station = state.stations.get(station_id)?;
    let module = &station.modules[module_idx];

    if !module.enabled || module.power_stalled {
        return None;
    }

    let def = content.module_defs.get(&module.def_id)?;

    let interval = match &def.behavior {
        crate::ModuleBehaviorDef::Processor(p) => p.processing_interval_ticks,
        crate::ModuleBehaviorDef::Assembler(a) => a.assembly_interval_ticks,
        crate::ModuleBehaviorDef::SensorArray(s) => s.scan_interval_ticks,
        crate::ModuleBehaviorDef::Lab(l) => l.research_interval_ticks,
        crate::ModuleBehaviorDef::Maintenance(m) => m.repair_interval_ticks,
        // Passive modules — no tick lifecycle
        crate::ModuleBehaviorDef::Storage { .. }
        | crate::ModuleBehaviorDef::SolarArray(_)
        | crate::ModuleBehaviorDef::Battery(_) => return None,
    };

    let efficiency = crate::wear::wear_efficiency(module.wear.wear, &content.constants);

    Some(ModuleTickContext {
        station_id,
        module_idx,
        module_id: &module.id,
        def,
        interval,
        power_needed: def.power_consumption_per_run,
        wear_per_run: def.wear_per_run,
        efficiency,
    })
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p sim_core extract_context`
Expected: All 4 tests PASS.

**Step 5: Commit**

```
feat(sim_core): add ModuleTickContext and extract_context() (VIO-172)
```

---

### Task 2: Add `should_run()` and `apply_run_result()`

**Files:**
- Modify: `crates/sim_core/src/station/mod.rs`

**Step 1: Write failing tests**

Add to the `framework_tests` module:

```rust
#[test]
fn should_run_returns_false_before_interval() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 2, // interval is 5, after increment = 3
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    assert!(!should_run(&mut state, &ctx));
}

#[test]
fn should_run_returns_true_at_interval() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 4, // after increment = 5 = interval
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    assert!(should_run(&mut state, &ctx));
}

#[test]
fn should_run_returns_false_when_insufficient_power() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 4, // would be at interval
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    // Set power below module needs (10.0)
    state.stations.get_mut(&station_id).unwrap().power_available_per_tick = 5.0;
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    assert!(!should_run(&mut state, &ctx));
}

#[test]
fn apply_run_result_completed_resets_timer_and_applies_wear() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();
    apply_run_result(&mut state, &ctx, RunOutcome::Completed, &mut events);

    let station = state.stations.get(&station_id).unwrap();
    // Timer reset
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert_eq!(ps.ticks_since_last_run, 0);
    }
    // Wear applied (0.01)
    assert!((station.modules[0].wear.wear - 0.01).abs() < 1e-6);
    // WearAccumulated event
    assert!(events.iter().any(|e| matches!(&e.event, Event::WearAccumulated { .. })));
}

#[test]
fn apply_run_result_skipped_keep_does_not_reset_timer_or_wear() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();
    apply_run_result(&mut state, &ctx, RunOutcome::Skipped { reset_timer: false }, &mut events);

    let station = state.stations.get(&station_id).unwrap();
    // Timer NOT reset
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert_eq!(ps.ticks_since_last_run, 5);
    }
    // No wear
    assert!((station.modules[0].wear.wear).abs() < 1e-6);
    // No events
    assert!(events.is_empty());
}

#[test]
fn apply_run_result_skipped_reset_resets_timer_but_no_wear() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: false,
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();
    apply_run_result(&mut state, &ctx, RunOutcome::Skipped { reset_timer: true }, &mut events);

    let station = state.stations.get(&station_id).unwrap();
    // Timer reset
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert_eq!(ps.ticks_since_last_run, 0);
    }
    // No wear
    assert!((station.modules[0].wear.wear).abs() < 1e-6);
    // No events
    assert!(events.is_empty());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core should_run`
Expected: FAIL — `should_run` not found.

**Step 3: Implement `should_run()` and `apply_run_result()`**

```rust
/// Increment timer and check if the module should run this tick.
/// Returns true if: timer >= interval AND station has enough power.
fn should_run(state: &mut GameState, ctx: &ModuleTickContext) -> bool {
    // Increment timer
    let ticks = {
        let Some(station) = state.stations.get_mut(ctx.station_id) else { return false };
        let module = &mut station.modules[ctx.module_idx];
        match &mut module.kind_state {
            ModuleKindState::Processor(s) => { s.ticks_since_last_run += 1; s.ticks_since_last_run }
            ModuleKindState::Assembler(s) => { s.ticks_since_last_run += 1; s.ticks_since_last_run }
            ModuleKindState::SensorArray(s) => { s.ticks_since_last_run += 1; s.ticks_since_last_run }
            ModuleKindState::Lab(s) => { s.ticks_since_last_run += 1; s.ticks_since_last_run }
            ModuleKindState::Maintenance(s) => { s.ticks_since_last_run += 1; s.ticks_since_last_run }
            _ => return false,
        }
    };

    if ticks < ctx.interval {
        return false;
    }

    // Check power
    let Some(station) = state.stations.get(ctx.station_id) else { return false };
    station.power_available_per_tick >= ctx.power_needed
}

/// Apply the outcome of a module run: timer reset, wear, volume cache.
fn apply_run_result(
    state: &mut GameState,
    ctx: &ModuleTickContext,
    outcome: RunOutcome,
    events: &mut Vec<EventEnvelope>,
) {
    match outcome {
        RunOutcome::Completed => {
            // Reset timer
            reset_timer_fn(state, ctx);
            // Apply wear
            apply_wear(state, ctx.station_id, ctx.module_idx, ctx.wear_per_run, events);
            // Invalidate volume cache (inventory may have changed)
            if let Some(station) = state.stations.get_mut(ctx.station_id) {
                station.invalidate_volume_cache();
            }
        }
        RunOutcome::Skipped { reset_timer } => {
            if reset_timer {
                reset_timer_fn(state, ctx);
            }
            // No wear on skipped modules
        }
        RunOutcome::Stalled(_reason) => {
            // Reset timer
            reset_timer_fn(state, ctx);
            // No wear on stalled modules
        }
    }
}

/// Reset the ticks_since_last_run to 0 for any module kind.
fn reset_timer_fn(state: &mut GameState, ctx: &ModuleTickContext) {
    let Some(station) = state.stations.get_mut(ctx.station_id) else { return };
    let module = &mut station.modules[ctx.module_idx];
    match &mut module.kind_state {
        ModuleKindState::Processor(s) => s.ticks_since_last_run = 0,
        ModuleKindState::Assembler(s) => s.ticks_since_last_run = 0,
        ModuleKindState::SensorArray(s) => s.ticks_since_last_run = 0,
        ModuleKindState::Lab(s) => s.ticks_since_last_run = 0,
        ModuleKindState::Maintenance(s) => s.ticks_since_last_run = 0,
        _ => {}
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p sim_core framework_tests`
Expected: All tests PASS.

**Step 5: Commit**

```
feat(sim_core): add should_run() and apply_run_result() framework (VIO-172)
```

---

### Task 3: Add stall transition helpers

**Files:**
- Modify: `crates/sim_core/src/station/mod.rs`

**Step 1: Write failing tests**

Add to `framework_tests`:

```rust
#[test]
fn stall_transition_emits_module_stalled_event() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: false, // not currently stalled
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();

    apply_run_result(
        &mut state, &ctx,
        RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
        &mut events,
    );

    // Should set stalled=true
    let station = state.stations.get(&station_id).unwrap();
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "should be stalled after VolumeCap");
    }
    // Should emit ModuleStalled
    assert!(events.iter().any(|e| matches!(&e.event, Event::ModuleStalled { .. })));
}

#[test]
fn stall_does_not_re_emit_when_already_stalled() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: true, // already stalled
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();

    apply_run_result(
        &mut state, &ctx,
        RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
        &mut events,
    );

    // No ModuleStalled event (already stalled)
    assert!(!events.iter().any(|e| matches!(&e.event, Event::ModuleStalled { .. })));
}

#[test]
fn completed_after_stall_emits_resumed_and_clears_flag() {
    let content = test_content_with_processor();
    let mut state = test_state_with_module(&content, ModuleKindState::Processor(ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 5,
        stalled: true, // was stalled
    }));
    let station_id = StationId("station_test".to_string());
    let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
    let mut events = Vec::new();

    apply_run_result(&mut state, &ctx, RunOutcome::Completed, &mut events);

    // Should clear stalled
    let station = state.stations.get(&station_id).unwrap();
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(!ps.stalled, "should be un-stalled after Completed");
    }
    // Should emit ModuleResumed
    assert!(events.iter().any(|e| matches!(&e.event, Event::ModuleResumed { .. })));
}
```

**Step 2: Run tests — expect FAIL**

Run: `cargo test -p sim_core stall_transition`

**Step 3: Expand `apply_run_result()` with stall transition logic**

Update the Stalled and Completed arms to read `was_stalled` from kind_state, set the flag, and emit transition events. Use helper functions `get_stall_flag()` and `set_stall_flag()` that match on `ModuleKindState` variants to read/write the appropriate field (`stalled` for Processor/Assembler, `starved` for Lab, etc.).

Key implementation details:
- `VolumeCap` maps to `stalled` field on Processor/Assembler
- `StockCap` maps to `capped` field on Assembler
- `DataStarved` maps to `starved` field on Lab
- Events: `VolumeCap` -> `ModuleStalled`/`ModuleResumed`, `StockCap` -> `AssemblerCapped`/`AssemblerUncapped`, `DataStarved` -> `LabStarved`/`LabResumed`

**Step 4: Run tests — expect PASS**

Run: `cargo test -p sim_core framework_tests`

**Step 5: Commit**

```
feat(sim_core): add stall transition handling to apply_run_result (VIO-172)
```

---

### Task 4: Refactor sensor module to use framework

Start with sensor — it's the simplest module (no stall logic, no inventory).

**Files:**
- Modify: `crates/sim_core/src/station/sensor.rs`
- Modify: `crates/sim_core/src/station/mod.rs` (add sensor to tick dispatch)

**Step 1: Verify existing tests pass (baseline)**

Run: `cargo test -p sim_core sensor`
Expected: All 3 sensor tests PASS.

**Step 2: Refactor `tick_sensor_array_modules()` to use framework**

Replace the function body with:

```rust
pub(super) fn tick_sensor_array_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let module_count = state.stations.get(station_id).map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        // Only process sensor arrays
        let crate::ModuleBehaviorDef::SensorArray(sensor_def) = &ctx.def.behavior else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, sensor_def, state, content, events);
        super::apply_run_result(state, &ctx, outcome, events);
    }
}

fn execute(
    ctx: &super::ModuleTickContext,
    sensor_def: &crate::SensorArrayDef,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    let current_tick = state.meta.tick;

    let amount = generate_data(
        &mut state.research,
        sensor_def.data_kind.clone(),
        &sensor_def.action_key,
        &content.constants,
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: sensor_def.data_kind.clone(),
            amount,
        },
    ));

    super::RunOutcome::Completed
}
```

Note: The old code reset the timer BEFORE the run (line 73 in original). The framework resets AFTER. This is a semantic change but functionally equivalent — the timer goes to 0 either way and the run still happens. The existing tests will catch any behavioral difference.

**Step 3: Run all sensor tests**

Run: `cargo test -p sim_core sensor`
Expected: All 3 tests PASS.

**Step 4: Run full sim_core test suite**

Run: `cargo test -p sim_core`
Expected: All tests PASS.

**Step 5: Commit**

```
refactor(sim_core): sensor module uses shared framework (VIO-172)
```

---

### Task 5: Refactor maintenance module to use framework

Second simplest — no stall logic, but has kit consumption and target finding.

**Files:**
- Modify: `crates/sim_core/src/station/maintenance.rs`

**Step 1: Verify existing tests pass**

Run: `cargo test -p sim_core maintenance`

**Step 2: Refactor `tick_maintenance_modules()`**

Replace with framework pattern. Key details:
- Maintenance has several "skip with timer reset" paths (no worn target, no kit). Return `RunOutcome::Skipped { reset_timer: true }` for these.
- Maintenance does NOT call `apply_wear()` on itself — it reduces wear on OTHER modules. So `execute()` returns `RunOutcome::Completed` but the framework's wear application still correctly applies wear_per_run to the maintenance module itself (if wear_per_run > 0; currently maintenance modules have wear_per_run=0 in content, so this is a no-op but correct).
- `MaintenanceRan` event is emitted inside `execute()`.

**Step 3: Run tests**

Run: `cargo test -p sim_core maintenance`
Then: `cargo test -p sim_core`

**Step 4: Commit**

```
refactor(sim_core): maintenance module uses shared framework (VIO-172)
```

---

### Task 6: Refactor lab module to use framework

Has stall logic (`starved` flag) — first module to use `StallReason::DataStarved`.

**Files:**
- Modify: `crates/sim_core/src/station/lab.rs`

**Step 1: Verify existing tests pass**

Run: `cargo test -p sim_core lab`

**Step 2: Refactor `tick_lab_modules()`**

Key details:
- "No tech assigned" and "tech already unlocked" are skip paths — return `Skipped { reset_timer: true }`.
- "No data available" returns `Stalled(DataStarved)`. Framework handles the `starved` flag and `LabStarved`/`LabResumed` events.
- `execute()` uses `ctx.efficiency` for research points (already in current code).

**Step 3: Run tests**

Run: `cargo test -p sim_core lab`
Then: `cargo test -p sim_core`

**Step 4: Commit**

```
refactor(sim_core): lab module uses shared framework (VIO-172)
```

---

### Task 7: Refactor processor module to use framework

Most complex — has capacity stall, ore threshold, FIFO consumption.

**Files:**
- Modify: `crates/sim_core/src/station/processor.rs`

**Step 1: Verify existing tests pass**

Run: `cargo test -p sim_core processor`

**Step 2: Refactor `tick_station_modules()`**

Key details:
- Ore threshold check below threshold: `continue` (no timer reset in current code — return `Skipped { reset_timer: false }`).
- Capacity pre-check: return `Stalled(VolumeCap { shortfall_m3 })`. Framework handles `stalled` flag and `ModuleStalled`/`ModuleResumed` events.
- `resolve_processor_run()` stays mostly as-is but called from `execute()`.
- `execute()` uses `ctx.efficiency` for wear efficiency (already computed in context).
- Current code does timer reset AND `resolve_processor_run()` separately. In the new version, `execute()` calls `resolve_processor_run()` and returns `Completed`. Framework resets timer and applies wear. Remove the `apply_wear()` call from `resolve_processor_run()` since framework handles it.

**Important:** `resolve_processor_run()` currently calls `apply_wear()` at the end. Remove that call — framework handles wear via `apply_run_result()`. But verify `wear_per_run` is read from the def (it is — line 390-393 in current code). The framework's `ctx.wear_per_run` provides this.

**Step 3: Run tests**

Run: `cargo test -p sim_core processor`
Then: `cargo test -p sim_core`

**Step 4: Commit**

```
refactor(sim_core): processor module uses shared framework (VIO-172)
```

---

### Task 8: Refactor assembler module to use framework

Most complex overall — stall, capping, tech gate, ship construction.

**Files:**
- Modify: `crates/sim_core/src/station/assembler.rs`

**Step 1: Verify existing tests pass**

Run: `cargo test -p sim_core assembler`

**Step 2: Refactor `tick_assembler_modules()`**

Key details:
- Insufficient inputs: return `Skipped { reset_timer: true }`. (Current code resets timer on line 121.)
- Stock cap: return `Stalled(StockCap)`. Framework handles `capped` flag and `AssemblerCapped`/`AssemblerUncapped` events.
- Volume cap: return `Stalled(VolumeCap { shortfall_m3 })`. Framework handles `stalled` flag and `ModuleStalled`/`ModuleResumed` events.
- Tech gate (ship construction): current code has special "don't reset timer" behavior to emit `ModuleAwaitingTech` only once. Return `Skipped { reset_timer: false }` — framework respects this.
- `resolve_assembler_run()` stays mostly as-is. Remove `apply_wear()` call at end — framework handles it.
- Assembler needs `rng` — `tick_assembler_modules()` signature includes it. Pass through to `execute()`.

**Step 3: Run tests**

Run: `cargo test -p sim_core assembler`
Then: `cargo test -p sim_core`

**Step 4: Commit**

```
refactor(sim_core): assembler module uses shared framework (VIO-172)
```

---

### Task 9: Final verification and cleanup

**Files:**
- Modify: `crates/sim_core/src/station/mod.rs` (cleanup any dead code)

**Step 1: Run full test suite**

Run: `cargo test -p sim_core`
Then: `cargo test` (all crates)
Then: `cargo clippy`

**Step 2: Run sim_bench to verify deterministic output**

Run: `cargo run -p sim_bench -- run --scenario scenarios/baseline.json`

Compare output against a baseline run on `main` to verify identical behavior. The simulation should produce exactly the same results since this is a pure refactor.

**Step 3: Remove any `#[allow(clippy::too_many_lines)]` attributes that are no longer needed**

The refactored `execute()` functions should be shorter than the original tick functions.

**Step 4: Verify no TODO stubs introduced**

Run: `grep -r "TODO" crates/sim_core/src/station/`

**Step 5: Commit cleanup**

```
chore(sim_core): cleanup after module framework refactor (VIO-172)
```

---

## Dependency Order

```
Task 1 (extract_context)
  └→ Task 2 (should_run + apply_run_result)
       └→ Task 3 (stall transitions)
            └→ Task 4 (sensor) — simplest, proves framework works
            └→ Task 5 (maintenance) — no stalls
            └→ Task 6 (lab) — DataStarved stall
            └→ Task 7 (processor) — VolumeCap stall
            └→ Task 8 (assembler) — VolumeCap + StockCap + tech gate
                 └→ Task 9 (final verification)
```

Tasks 4-8 can be done in any order after Task 3, but the recommended order (sensor → maintenance → lab → processor → assembler) goes from simplest to most complex.
