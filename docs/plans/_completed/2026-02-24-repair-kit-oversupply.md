# Repair Kit Oversupply Fix (VIO-15) — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce repair kit oversupply by slowing assembler production, increasing material cost, and adding a content-driven stock cap with command override.

**Architecture:** Three changes to the assembler system: (1) tune `module_defs.json` values (interval + cost), (2) add `max_stock` field to `AssemblerDef` and cap-check logic in `tick_assembler_modules`, (3) add `SetAssemblerCap` command and `cap_override` on `AssemblerState` for runtime adjustments. The bench override system also needs a new `max_stock` override key.

**Tech Stack:** Rust (sim_core, sim_bench), JSON content files

---

### Task 1: Add `max_stock` to `AssemblerDef` and `cap_override` to `AssemblerState`

**Files:**
- Modify: `crates/sim_core/src/types.rs:608-611` (AssemblerDef)
- Modify: `crates/sim_core/src/types.rs:197-202` (AssemblerState)

**Step 1: Add `max_stock` field to `AssemblerDef`**

In `types.rs`, add to the `AssemblerDef` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerDef {
    pub assembly_interval_ticks: u64,
    pub recipes: Vec<RecipeDef>,
    #[serde(default)]
    pub max_stock: HashMap<ComponentId, u32>,
}
```

`HashMap` is already imported. `ComponentId` is already defined in types. `serde(default)` means existing content without `max_stock` gets an empty map (no cap).

**Step 2: Add `cap_override` field to `AssemblerState`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerState {
    pub ticks_since_last_run: u64,
    #[serde(default)]
    pub stalled: bool,
    #[serde(default)]
    pub capped: bool,
    #[serde(default)]
    pub cap_override: HashMap<ComponentId, u32>,
}
```

`capped` tracks whether the assembler is currently at-cap (for event dedup, like `stalled`). `cap_override` stores runtime overrides from the `SetAssemblerCap` command — if a component key is present here, it takes priority over the content def's `max_stock`.

**Step 3: Verify it compiles**

Run: `cargo build -p sim_core`
Expected: compiles (all existing code uses `AssemblerState { ticks_since_last_run, stalled }` — the new `#[serde(default)]` fields are backwards-compatible). Fix any compilation errors from pattern matching on `AssemblerState` if they appear — the existing test helper at `tests/mod.rs:212-214` constructs `AssemblerState` with named fields and will need `capped: false, cap_override: HashMap::new()` added.

**Step 4: Commit**

```
feat(types): add max_stock to AssemblerDef and cap_override to AssemblerState
```

---

### Task 2: Add `SetAssemblerCap` command and `AssemblerCapped`/`AssemblerUncapped` events

**Files:**
- Modify: `crates/sim_core/src/types.rs:309` (Command enum)
- Modify: `crates/sim_core/src/types.rs:351` (Event enum)
- Modify: `crates/sim_core/src/engine.rs:232-246` (apply_commands, after the AssignLabTech arm)

**Step 1: Add the command variant**

In the `Command` enum (types.rs:309):

```rust
SetAssemblerCap {
    station_id: StationId,
    module_id: ModuleInstanceId,
    component_id: ComponentId,
    max_stock: u32,
},
```

**Step 2: Add event variants**

In the `Event` enum (types.rs:351), add after `AssemblerRan`:

```rust
AssemblerCapped {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
AssemblerUncapped {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
```

**Step 3: Handle the command in `apply_commands`**

In `engine.rs`, add a new match arm in `apply_commands` after `Command::AssignLabTech`:

```rust
Command::SetAssemblerCap {
    station_id,
    module_id,
    component_id,
    max_stock,
} => {
    let Some(station) = state.stations.get_mut(station_id) else {
        continue;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        continue;
    };
    if let crate::ModuleKindState::Assembler(asmb) = &mut module.kind_state {
        asmb.cap_override.insert(component_id.clone(), *max_stock);
    }
}
```

**Step 4: Verify it compiles**

Run: `cargo build -p sim_core`
Expected: compiles. The new Event variants need to be handled in any exhaustive matches — check if there are any (events are typically matched with `..` patterns). Also check `sim_daemon` for SSE event serialization if it pattern-matches on `Event`.

**Step 5: Commit**

```
feat(core): add SetAssemblerCap command and AssemblerCapped/Uncapped events
```

---

### Task 3: Implement cap-check logic in `tick_assembler_modules`

**Files:**
- Modify: `crates/sim_core/src/station.rs:499-696` (tick_assembler_modules)

**Step 1: Write the failing test**

In `crates/sim_core/src/tests/assembler.rs`, add:

```rust
#[test]
fn test_assembler_stops_at_max_stock() {
    let mut content = assembler_content();
    // Set max_stock to 2 repair kits
    if let ModuleBehaviorDef::Assembler(ref mut asm_def) = content.module_defs[0].behavior {
        asm_def.max_stock.insert(ComponentId("repair_kit".to_string()), 2);
    }
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Pre-seed with 2 repair kits (at cap)
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 2,
            quality: 1.0,
        });

    let mut rng = make_rng();
    // Tick past interval
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Assembler should NOT have run
    assert!(
        !events.iter().any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "assembler should not run when at max_stock"
    );

    // AssemblerCapped event should fire
    assert!(
        events.iter().any(|e| matches!(e.event, Event::AssemblerCapped { .. })),
        "AssemblerCapped event should be emitted"
    );

    // Kit count should still be 2
    let kit_count: u32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Component { component_id, count, .. }
                if component_id.0 == "repair_kit" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(kit_count, 2);
}

#[test]
fn test_assembler_resumes_below_max_stock() {
    let mut content = assembler_content();
    if let ModuleBehaviorDef::Assembler(ref mut asm_def) = content.module_defs[0].behavior {
        asm_def.max_stock.insert(ComponentId("repair_kit".to_string()), 3);
    }
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Pre-seed with 2 kits (below cap of 3)
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 2,
            quality: 1.0,
        });

    // Mark as previously capped
    if let ModuleKindState::Assembler(asmb) = &mut state.stations.get_mut(&station_id).unwrap().modules[0].kind_state {
        asmb.capped = true;
    }

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Assembler SHOULD run (2 < 3)
    assert!(
        events.iter().any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "assembler should run when below max_stock"
    );

    // AssemblerUncapped event should fire
    assert!(
        events.iter().any(|e| matches!(e.event, Event::AssemblerUncapped { .. })),
        "AssemblerUncapped event should be emitted"
    );
}

#[test]
fn test_assembler_cap_override_takes_priority() {
    let mut content = assembler_content();
    // Content cap: 10
    if let ModuleBehaviorDef::Assembler(ref mut asm_def) = content.module_defs[0].behavior {
        asm_def.max_stock.insert(ComponentId("repair_kit".to_string()), 10);
    }
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Override cap to 2
    if let ModuleKindState::Assembler(asmb) = &mut state.stations.get_mut(&station_id).unwrap().modules[0].kind_state {
        asmb.cap_override.insert(ComponentId("repair_kit".to_string()), 2);
    }

    // Pre-seed with 2 kits (at override cap, below content cap)
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 2,
            quality: 1.0,
        });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Assembler should NOT run (override cap of 2 takes priority)
    assert!(
        !events.iter().any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "assembler should respect cap_override over content max_stock"
    );
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core test_assembler_stops_at_max_stock test_assembler_resumes_below_max_stock test_assembler_cap_override_takes_priority`
Expected: FAIL (no cap logic implemented yet)

**Step 3: Implement cap check in `tick_assembler_modules`**

In `station.rs`, inside `tick_assembler_modules`, after the input availability check (line ~589) and before the capacity pre-check (line ~601), add the stock cap check:

```rust
// Stock cap check: skip if any output component is at or above max_stock
let at_cap = {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let effective_caps: &HashMap<ComponentId, u32> = {
        // Determine caps: override > content def
        // We need to build the effective map
        &HashMap::new() // placeholder, see below
    };
    // Actually: get caps from assembler state (override) merged with def
    let (cap_override, was_capped) = match &station.modules[module_idx].kind_state {
        ModuleKindState::Assembler(asmb) => (&asmb.cap_override, asmb.capped),
        _ => continue,
    };

    let mut is_capped = false;
    for output in &recipe.outputs {
        if let OutputSpec::Component { component_id, .. } = output {
            // Effective cap: override if present, else content def
            let cap = cap_override
                .get(component_id)
                .or_else(|| assembler_def.max_stock.get(component_id));
            if let Some(&max) = cap {
                let current: u32 = station
                    .inventory
                    .iter()
                    .filter_map(|i| match i {
                        InventoryItem::Component {
                            component_id: cid,
                            count,
                            ..
                        } if cid == component_id => Some(*count),
                        _ => None,
                    })
                    .sum();
                if current >= max {
                    is_capped = true;
                    break;
                }
            }
        }
    }
    (is_capped, was_capped)
};

// Handle cap state transitions and skip if capped
let (is_capped, was_capped) = at_cap;
if is_capped {
    if let Some(station) = state.stations.get_mut(station_id) {
        if let ModuleKindState::Assembler(asmb) = &mut station.modules[module_idx].kind_state {
            if !asmb.capped {
                asmb.capped = true;
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::AssemblerCapped {
                        station_id: station_id.clone(),
                        module_id: module_id.clone(),
                    },
                ));
            }
            asmb.ticks_since_last_run = 0;
        }
    }
    continue;
}
// Uncap if previously capped
if was_capped {
    if let Some(station) = state.stations.get_mut(station_id) {
        if let ModuleKindState::Assembler(asmb) = &mut station.modules[module_idx].kind_state {
            asmb.capped = false;
        }
    }
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::AssemblerUncapped {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
        },
    ));
}
```

Note: The pseudo-code above shows the intent. The implementer should restructure it to avoid borrowing issues (extract what's needed from `state` immutably first, then mutate). Follow the same borrow-split pattern used elsewhere in this function.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p sim_core assembler`
Expected: all assembler tests PASS including the 3 new ones. Existing tests still pass because they don't set `max_stock` (empty HashMap = no cap).

**Step 5: Commit**

```
feat(station): implement assembler max_stock cap check with event transitions
```

---

### Task 4: Update content files (interval, cost, max_stock)

**Files:**
- Modify: `content/module_defs.json:60-91` (assembler definition)

**Step 1: Update module_defs.json**

Change the `module_basic_assembler` entry:

```json
{
    "id": "module_basic_assembler",
    "name": "Basic Assembler",
    "mass_kg": 3000.0,
    "volume_m3": 8.0,
    "power_consumption_per_run": 8.0,
    "wear_per_run": 0.008,
    "behavior": {
      "Assembler": {
        "assembly_interval_ticks": 360,
        "recipes": [
          {
            "id": "recipe_basic_repair_kit",
            "inputs": [
              {
                "filter": { "Element": "Fe" },
                "amount": { "Kg": 200.0 }
              }
            ],
            "outputs": [
              {
                "Component": {
                  "component_id": "repair_kit",
                  "quality_formula": { "Fixed": 1.0 }
                }
              }
            ],
            "efficiency": 1.0
          }
        ],
        "max_stock": { "repair_kit": 50 }
      }
    }
  }
```

Changes: `assembly_interval_ticks` 120→360, input `Kg` 100→200, added `max_stock`.

**Step 2: Run full test suite**

Run: `cargo test`
Expected: all tests pass. The test helper `assembler_content()` in `tests/mod.rs` uses its own values (interval=2, cost=100kg) so it's not affected.

**Step 3: Commit**

```
balance(content): slow assembler to 6h interval, 200kg cost, 50-kit stock cap
```

---

### Task 5: Update bench override system for new assembler fields

**Files:**
- Modify: `crates/sim_bench/src/overrides.rs:41-48` (assembler override arm)

**Step 1: Add override support for `max_stock`**

In the assembler arm of `apply_module_override`, add a case for `max_stock`:

```rust
(ModuleBehaviorDef::Assembler(ref mut asm_def), "assembler") => {
    match field {
        "assembly_interval_ticks" => asm_def.assembly_interval_ticks = as_u64(full_key, value)?,
        "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
        "max_stock" => {
            // Expect value like {"repair_kit": 50}
            let map: HashMap<String, u32> = serde_json::from_value(value.clone())
                .with_context(|| format!("invalid max_stock value for '{full_key}'"))?;
            asm_def.max_stock = map
                .into_iter()
                .map(|(k, v)| (ComponentId(k), v))
                .collect();
        }
        _ => bail!("unknown assembler field '{field}' in override key '{full_key}'. Valid fields: assembly_interval_ticks, wear_per_run, max_stock"),
    }
    matched = true;
}
```

**Step 2: Add a test for the new override**

In the test module of `overrides.rs`:

```rust
#[test]
fn test_module_assembler_max_stock_override() {
    let mut content = test_content();
    let overrides = HashMap::from([(
        "module.assembler.max_stock".to_string(),
        serde_json::json!({"repair_kit": 25}),
    )]);
    apply_overrides(&mut content, &overrides).unwrap();

    for module_def in &content.module_defs {
        if let ModuleBehaviorDef::Assembler(ref asm_def) = module_def.behavior {
            assert_eq!(
                asm_def.max_stock.get(&ComponentId("repair_kit".to_string())),
                Some(&25)
            );
        }
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p sim_bench`
Expected: PASS

**Step 4: Commit**

```
feat(bench): add max_stock override support for assembler
```

---

### Task 6: Run benchmark scenarios and verify improvement

**Step 1: Run baseline scenario**

Run: `cargo run -p sim_bench -- run --scenario scenarios/baseline.json`

Check `batch_summary.json` for `repair_kits_remaining` — should be capped around 50.

**Step 2: Run month and quarter scenarios**

Run: `cargo run -p sim_bench -- run --scenario scenarios/month.json`
Run: `cargo run -p sim_bench -- run --scenario scenarios/quarter.json`

Both should show `repair_kits_remaining` around 50, not 149 or 694.

**Step 3: Verify no regressions**

Check that modules don't starve for repair kits (wear shouldn't climb to critical). Check refinery throughput is still healthy.

**Step 4: Commit any scenario file updates if needed**

---

### Task 7: Update docs and Linear issue

**Files:**
- Modify: `docs/reference.md` — update assembler section with max_stock, new interval/cost, new command/events

**Step 1: Update reference.md**

Add `max_stock` to the assembler behavior def documentation. Document the `SetAssemblerCap` command. Document `AssemblerCapped`/`AssemblerUncapped` events. Update the assembler interval and recipe cost values.

**Step 2: Update VIO-15 in Linear**

Add a comment with the benchmark results. Move issue to Done.

**Step 3: Commit**

```
docs: update reference.md with assembler cap, new interval/cost (VIO-15)
```
