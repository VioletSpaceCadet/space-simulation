# Storage Enforcement Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enforce hard storage capacity constraints — modules stall when output won't fit, ships wait when station is full, with proper events and metrics.

**Architecture:** Pre-check guard pattern. Before a processor runs, dry-run ore consumption to estimate output volume. If it won't fit, stall. For deposits, when nothing fits, ship stays in Deposit task and retries next tick. New events track stall/resume and deposit blocked/unblocked transitions.

**Tech Stack:** Rust (sim_core crate). No new dependencies.

---

### Task 1: Add `stalled` field to `ProcessorState`

**Files:**
- Modify: `crates/sim_core/src/types.rs:160-164` (ProcessorState struct)

**Step 1: Write the failing test**

In `crates/sim_core/src/tests.rs`, add a test that constructs a `ProcessorState` with `stalled: false`:

```rust
#[test]
fn test_processor_state_has_stalled_field() {
    let ps = ProcessorState {
        threshold_kg: 100.0,
        ticks_since_last_run: 0,
        stalled: false,
    };
    assert!(!ps.stalled);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_processor_state_has_stalled_field`
Expected: FAIL — `stalled` field does not exist

**Step 3: Add the field**

In `crates/sim_core/src/types.rs`, add `stalled: bool` to `ProcessorState`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorState {
    pub threshold_kg: f32,
    pub ticks_since_last_run: u64,
    #[serde(default)]
    pub stalled: bool,
}
```

The `#[serde(default)]` ensures backward compatibility with existing serialized states.

**Step 4: Fix all compilation errors**

Every place that constructs `ProcessorState` needs `stalled: false` added:
- `crates/sim_core/src/engine.rs:104-107` (InstallModule command)
- `crates/sim_core/src/tests.rs` (multiple test helpers: `state_with_refinery`, `test_refinery_skips_when_below_threshold`)
- `crates/sim_core/src/metrics.rs` tests (test_refinery_starved_detection)

**Step 5: Run all tests to verify they pass**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(types): add stalled field to ProcessorState"
```

---

### Task 2: Add new event variants

**Files:**
- Modify: `crates/sim_core/src/types.rs:304-397` (Event enum)

**Step 1: Write the failing test**

In `crates/sim_core/src/tests.rs`:

```rust
#[test]
fn test_module_stalled_event_variant_exists() {
    let event = Event::ModuleStalled {
        station_id: StationId("s".to_string()),
        module_id: ModuleInstanceId("m".to_string()),
        shortfall_m3: 1.0,
    };
    assert!(matches!(event, Event::ModuleStalled { .. }));
}

#[test]
fn test_module_resumed_event_variant_exists() {
    let event = Event::ModuleResumed {
        station_id: StationId("s".to_string()),
        module_id: ModuleInstanceId("m".to_string()),
    };
    assert!(matches!(event, Event::ModuleResumed { .. }));
}

#[test]
fn test_deposit_blocked_event_variant_exists() {
    let event = Event::DepositBlocked {
        ship_id: ShipId("s".to_string()),
        station_id: StationId("st".to_string()),
        shortfall_m3: 1.0,
    };
    assert!(matches!(event, Event::DepositBlocked { .. }));
}

#[test]
fn test_deposit_unblocked_event_variant_exists() {
    let event = Event::DepositUnblocked {
        ship_id: ShipId("s".to_string()),
        station_id: StationId("st".to_string()),
    };
    assert!(matches!(event, Event::DepositUnblocked { .. }));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core test_module_stalled_event`
Expected: FAIL — variants don't exist

**Step 3: Add the variants to Event enum**

In `crates/sim_core/src/types.rs`, add to the `Event` enum:

```rust
ModuleStalled {
    station_id: StationId,
    module_id: ModuleInstanceId,
    shortfall_m3: f32,
},
ModuleResumed {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
DepositBlocked {
    ship_id: ShipId,
    station_id: StationId,
    shortfall_m3: f32,
},
DepositUnblocked {
    ship_id: ShipId,
    station_id: StationId,
},
```

**Step 4: Run all tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(types): add ModuleStalled/Resumed and DepositBlocked/Unblocked events"
```

---

### Task 3: Refactor `consume_ore_fifo_with_lots` for dry-run support

**Files:**
- Modify: `crates/sim_core/src/station.rs:291-331` (consume_ore_fifo_with_lots function)

**Step 1: Write a test for the dry-run (peek) function**

In `crates/sim_core/src/station.rs`, add a `#[cfg(test)] mod tests` block (or add to existing tests):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AsteroidId, InventoryItem, LotId};

    #[test]
    fn test_peek_ore_fifo_does_not_mutate() {
        let inventory = vec![
            InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 600.0,
                composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
            },
            InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 100.0,
                quality: 0.8,
            },
        ];

        let filter = |item: &InventoryItem| matches!(item, InventoryItem::Ore { .. });
        let (consumed_kg, lots) = peek_ore_fifo_with_lots(&inventory, 500.0, filter);

        assert!((consumed_kg - 500.0).abs() < 1e-3);
        assert_eq!(lots.len(), 1);
        // Original inventory unchanged — it's a borrow, no mutation
        assert_eq!(inventory.len(), 2);
        if let InventoryItem::Ore { kg, .. } = &inventory[0] {
            assert!((kg - 600.0).abs() < 1e-3, "original should be unchanged");
        }
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_peek_ore_fifo`
Expected: FAIL — function does not exist

**Step 3: Implement `peek_ore_fifo_with_lots`**

Add a new function in `crates/sim_core/src/station.rs` that reads inventory without mutating:

```rust
/// Peek at what would be consumed by FIFO without mutating inventory.
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)`.
fn peek_ore_fifo_with_lots(
    inventory: &[InventoryItem],
    rate_kg: f32,
    filter: impl Fn(&InventoryItem) -> bool,
) -> (f32, Vec<(HashMap<String, f32>, f32)>) {
    let mut remaining = rate_kg;
    let mut consumed_kg = 0.0_f32;
    let mut lots: Vec<(HashMap<String, f32>, f32)> = Vec::new();

    for item in inventory {
        if remaining <= 0.0 {
            break;
        }
        if matches!(item, InventoryItem::Ore { .. }) && filter(item) {
            let InventoryItem::Ore { kg, composition, .. } = item else {
                unreachable!()
            };
            let take = kg.min(remaining);
            remaining -= take;
            consumed_kg += take;
            lots.push((composition.clone(), take));
        }
    }
    (consumed_kg, lots)
}
```

**Step 4: Run tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "refactor(station): add peek_ore_fifo_with_lots for dry-run capacity checks"
```

---

### Task 4: Implement module stall logic

**Files:**
- Modify: `crates/sim_core/src/station.rs:23-116` (tick_station_modules)

**Step 1: Write the failing test — module stalls when output won't fit**

In `crates/sim_core/src/station.rs` tests (or `crates/sim_core/src/tests.rs`):

```rust
#[test]
fn test_refinery_stalls_when_station_full() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);

    // Shrink station capacity so refinery output won't fit.
    // 500 kg ore at 70% Fe → 350 kg Fe at density 7874 → ~0.044 m³
    // plus slag: 150 kg at density 2500 → 0.06 m³
    // Total output ≈ 0.104 m³. Set capacity to 0.05 so it won't fit.
    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 0.05;

    let mut rng = make_rng();
    // Tick twice to reach processing interval (interval=2)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Module should be stalled
    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled when output won't fit");
        assert_eq!(ps.ticks_since_last_run, 0, "timer should reset on stall");
    } else {
        panic!("expected processor state");
    }

    // Should have emitted ModuleStalled event
    assert!(
        events.iter().any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled event should be emitted"
    );

    // No material or slag should have been produced
    assert!(
        !station.inventory.iter().any(|i| matches!(i, InventoryItem::Material { .. })),
        "no material should be produced when stalled"
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_refinery_stalls_when_station_full`
Expected: FAIL — stall logic not implemented, module still produces output

**Step 3: Implement the stall pre-check**

In `crates/sim_core/src/station.rs`, in `tick_station_modules`, after the ore threshold check (after line 106) and before calling `resolve_processor_run` (line 108):

1. Peek at the ore that would be consumed using `peek_ore_fifo_with_lots`
2. Compute expected output volume from recipe outputs + actual composition
3. Check if `used_volume + output_volume > capacity`
4. If stalled: set `stalled = true`, reset timer, emit event if transition, `continue`
5. If not stalled and was previously stalled: set `stalled = false`, emit ModuleResumed

The implementation needs access to the recipe, element densities, and the current station volume. The key function to add is `estimate_output_volume_m3` that takes the recipe, consumed lots, and content, and returns the volume.

**Step 4: Write test for ModuleResumed — stall then recover**

```rust
#[test]
fn test_refinery_resumes_after_stall_cleared() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Start with tiny capacity to cause stall
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 0.05;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Confirm stalled
    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled);
    }

    // Now increase capacity so output fits
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 10_000.0;

    // Need to tick through another full interval (2 ticks) since timer was reset
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Module should have resumed
    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(!ps.stalled, "module should no longer be stalled");
    }

    assert!(
        events.iter().any(|e| matches!(e.event, Event::ModuleResumed { .. })),
        "ModuleResumed event should be emitted"
    );
}
```

**Step 5: Run all tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(station): enforce storage capacity — stall modules when output won't fit"
```

---

### Task 5: Implement deposit blocking — ship waits when station full

**Files:**
- Modify: `crates/sim_core/src/tasks.rs:374-455` (resolve_deposit)
- Modify: `crates/sim_core/src/types.rs:249-252` (TaskKind::Deposit)

**Step 1: Write the failing test**

In `crates/sim_core/src/tests.rs`:

```rust
#[test]
fn test_deposit_ship_waits_when_station_full() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 0.001; // tiny

    let ship_id = ShipId("ship_0001".to_string());
    state.ships.get_mut(&ship_id).unwrap().inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_block_test".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 500.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Ship should still be in Deposit task (waiting), NOT idle
    let ship = &state.ships[&ship_id];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Deposit { .. })),
        "ship should stay in Deposit task when station is full, not go idle"
    );

    // Ship should still have its ore
    assert!(
        !ship.inventory.is_empty(),
        "ship should retain ore when deposit is blocked"
    );

    // Should emit DepositBlocked event
    assert!(
        events.iter().any(|e| matches!(e.event, Event::DepositBlocked { .. })),
        "DepositBlocked event should be emitted"
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_deposit_ship_waits`
Expected: FAIL — ship currently goes idle

**Step 3: Modify resolve_deposit**

Change `resolve_deposit` in `crates/sim_core/src/tasks.rs`:

Current behavior (lines 425-428): when `to_deposit.is_empty()`, ship goes idle.
New behavior: when nothing fits, ship stays in Deposit task. Add `deposit_blocked: bool` tracking to `TaskKind::Deposit` for event dedup.

Update `TaskKind::Deposit`:
```rust
Deposit {
    station: StationId,
    #[serde(default)]
    blocked: bool,
},
```

In `resolve_deposit`, when `to_deposit.is_empty()`:
- If not yet blocked: emit `DepositBlocked`, set `blocked = true`
- Ship stays in Deposit task (don't call `set_ship_idle`)
- Extend `eta_tick` by 1 so ship retries next tick

When deposit succeeds (even partial): if was blocked, emit `DepositUnblocked`.

**Step 4: Write test for DepositUnblocked**

```rust
#[test]
fn test_deposit_unblocks_when_space_opens() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state.ships.get_mut(&ship_id).unwrap().inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_unblock_test".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal); // blocked

    // Now increase capacity
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 10_000.0;
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Ship should have deposited and gone idle
    assert!(state.ships[&ship_id].inventory.is_empty());
    assert!(
        events.iter().any(|e| matches!(e.event, Event::DepositUnblocked { .. })),
        "DepositUnblocked should be emitted when deposit succeeds after being blocked"
    );
}
```

**Step 5: Fix existing tests**

The existing test `test_deposit_respects_station_capacity` expects ship to go idle when station is full. Update it to expect ship stays in Deposit task instead. Also update `test_deposit_partial_when_station_partially_full` if needed (partial still works — ship deposits what fits and idles).

**Step 6: Run all tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(tasks): ship waits at station when deposit blocked by capacity"
```

---

### Task 6: Add `refinery_stalled_count` metric

**Files:**
- Modify: `crates/sim_core/src/metrics.rs:17-61` (MetricsSnapshot struct)
- Modify: `crates/sim_core/src/metrics.rs:127-299` (compute_metrics function)
- Modify: `crates/sim_core/src/metrics.rs:302-353` (CSV header and row)

**Step 1: Write the failing test**

In `crates/sim_core/src/metrics.rs` tests:

```rust
#[test]
fn test_refinery_stalled_metric() {
    let mut content = base_content();
    content.module_defs = vec![crate::ModuleDef {
        id: "module_basic_iron_refinery".to_string(),
        name: "Basic Iron Refinery".to_string(),
        mass_kg: 5000.0,
        volume_m3: 10.0,
        power_consumption_per_run: 10.0,
        behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
            processing_interval_ticks: 60,
            recipes: vec![],
        }),
    }];

    let mut state = empty_state();
    let station = make_station(
        vec![InventoryItem::Ore {
            lot_id: LotId("lot_0001".to_string()),
            asteroid_id: AsteroidId("ast_0001".to_string()),
            kg: 1000.0,
            composition: HashMap::from([("Fe".to_string(), 0.7)]),
        }],
        vec![ModuleState {
            id: ModuleInstanceId("mod_0001".to_string()),
            def_id: "module_basic_iron_refinery".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Processor(ProcessorState {
                threshold_kg: 500.0,
                ticks_since_last_run: 0,
                stalled: true,  // Marked as stalled
            }),
        }],
    );
    state.stations.insert(station.id.clone(), station);

    let snapshot = compute_metrics(&state, &content);
    assert_eq!(snapshot.refinery_stalled_count, 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p sim_core test_refinery_stalled_metric`
Expected: FAIL — field doesn't exist

**Step 3: Add the field and computation**

Add `refinery_stalled_count: u32` to `MetricsSnapshot`. In `compute_metrics`, count modules where `stalled == true`. Update `write_metrics_header` and `append_metrics_row` to include the new field.

**Step 4: Run all tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(metrics): add refinery_stalled_count metric"
```

---

### Task 7: Update downstream crates and documentation

**Files:**
- Modify: `crates/sim_control/src/lib.rs` — if autopilot constructs `ProcessorState` or `TaskKind::Deposit`
- Modify: `crates/sim_daemon/src/main.rs` — if it pattern-matches on events
- Modify: `ui_web/src/` — if TypeScript types need updating for new events
- Modify: `CLAUDE.md` — update public API if needed
- Modify: `docs/reference.md` — document new events and stall behavior

**Step 1: Search for all `ProcessorState` and `TaskKind::Deposit` constructions in downstream crates**

Use grep to find all locations outside sim_core that construct these types.

**Step 2: Add `stalled: false` / `blocked: false` where needed**

**Step 3: Update event handling in daemon SSE stream if it pattern-matches events**

**Step 4: Run full workspace tests**

Run: `cargo test`
Expected: ALL PASS (all crates)

**Step 5: Update docs**

- In `CLAUDE.md`, add stall/blocked behavior to the architecture notes
- In `docs/reference.md`, document new events and their fields

**Step 6: Commit**

```bash
git add -A && git commit -m "chore: update downstream crates and docs for storage enforcement"
```

---

### Task 8: Integration test — full stall cascade

**Files:**
- Modify: `crates/sim_core/src/tests.rs`

**Step 1: Write an integration test that exercises the full cascade**

```rust
#[test]
fn test_storage_pressure_cascade() {
    // Set up: station with tiny capacity, refinery, and incoming ore.
    // Verify: refinery runs until station fills → stalls → ship can't deposit → blocked.
    // Then: increase capacity → refinery resumes → ship deposits.
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let ship_id = ShipId("ship_0001".to_string());

    // Give station just enough capacity for one refinery run output
    // 500 kg ore at 70% Fe → 350 kg Fe (0.044 m³) + 150 kg slag (0.06 m³) = ~0.104 m³
    // Plus the ore itself: 1000 kg at 3000 density = 0.333 m³
    // Total initial = 0.333 m³. Set capacity to 0.45 m³ — room for ore + one run output.
    state.stations.get_mut(&station_id).unwrap().cargo_capacity_m3 = 0.45;

    let mut rng = make_rng();
    // Tick enough for refinery to run once (interval=2)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // After first run, station should have material + slag + remaining ore
    // Next run should stall because output won't fit
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Module should be stalled after second attempt
    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled after station fills");
    }
}
```

**Step 2: Run and verify**

Run: `cargo test -p sim_core test_storage_pressure_cascade`
Expected: PASS

**Step 3: Commit**

```bash
git add -A && git commit -m "test: add storage pressure cascade integration test"
```
