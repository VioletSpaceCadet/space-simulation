# Refinery System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.
> Subagents MUST use the Write and Edit tools to modify files — never use cat with heredoc or echo redirection.

**Goal:** Add rich inventory, station modules, and a basic iron refinery that converts ore to Fe material + slag.

**Architecture:** Replace `cargo: HashMap<ElementId, f32>` on ships/stations with `inventory: Vec<InventoryItem>` (a tagged enum carrying per-item metadata). Station modules are physical inventory items auto-installed by the autopilot; the refinery runs every 60 ticks when enabled and above a threshold. A `dev_base_state.json` file provides the initial world state as the foundation for save/load.

**Tech Stack:** Rust (sim_core, sim_control, sim_cli), serde_json, TypeScript/React (ui_web).

**Test commands:**
- Rust: `cargo test` (run from repo root)
- Frontend: `cd ui_web && npm test -- --run`
- Type check: `cd ui_web && npx tsc --noEmit`

---

### Task 1: Add new types to `types.rs` (additive only)

**Files:**
- Modify: `crates/sim_core/src/types.rs`
- Modify: `crates/sim_core/src/lib.rs` (test helpers + export)
- Modify: `crates/sim_control/src/lib.rs` (test helper)

**Context:** This task only adds new types — it does not yet change `ShipState` or `StationState`. All existing tests must still pass after this task.

**Step 1: Add new ID newtypes and update `Counters`**

In `crates/sim_core/src/types.rs`, after the existing `string_id!` calls (after line 41), add:

```rust
string_id!(LotId);
string_id!(ModuleItemId);
string_id!(ModuleInstanceId);
string_id!(ComponentId);
```

Replace the `Counters` struct (lines 97–101) with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counters {
    pub next_event_id: u64,
    pub next_command_id: u64,
    pub next_asteroid_id: u64,
    pub next_lot_id: u64,
    pub next_module_instance_id: u64,
}
```

**Step 2: Add `InventoryItem` and module state types**

In `crates/sim_core/src/types.rs`, after the `Counters` struct, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InventoryItem {
    Ore {
        lot_id: LotId,
        asteroid_id: AsteroidId,
        kg: f32,
        composition: CompositionVec,
    },
    Slag {
        kg: f32,
        composition: CompositionVec,
    },
    Material {
        element: ElementId,
        kg: f32,
        quality: f32,
    },
    Component {
        component_id: ComponentId,
        count: u32,
        quality: f32,
    },
    Module {
        item_id: ModuleItemId,
        module_def_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleState {
    pub id: ModuleInstanceId,
    pub def_id: String,
    pub enabled: bool,
    pub kind_state: ModuleKindState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorState {
    pub threshold_kg: f32,
    pub ticks_since_last_run: u64,
}
```

**Step 3: Add module content types**

In `crates/sim_core/src/types.rs`, in the Content types section (after `Constants`), add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
    pub power_consumption_per_run: f32,
    pub behavior: ModuleBehaviorDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorDef {
    pub processing_interval_ticks: u64,
    pub recipes: Vec<RecipeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<OutputSpec>,
    pub efficiency: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeInput {
    pub filter: InputFilter,
    pub amount: InputAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputFilter {
    ItemKind(ItemKind),
    Element(ElementId),
    ElementWithMinQuality { element: ElementId, min_quality: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemKind {
    Ore,
    Slag,
    Material,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputAmount {
    Kg(f32),
    Count(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputSpec {
    Material {
        element: ElementId,
        yield_formula: YieldFormula,
        quality_formula: QualityFormula,
    },
    Slag {
        yield_formula: YieldFormula,
    },
    Component {
        component_id: ComponentId,
        quality_formula: QualityFormula,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YieldFormula {
    ElementFraction { element: ElementId },
    FixedFraction(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityFormula {
    ElementFractionTimesMultiplier { element: ElementId, multiplier: f32 },
    Fixed(f32),
}
```

**Step 4: Add `module_defs` to `GameContent` and `refined_name` to `ElementDef`**

Replace the `GameContent` struct with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameContent {
    pub content_version: String,
    pub techs: Vec<TechDef>,
    pub solar_system: SolarSystemDef,
    pub asteroid_templates: Vec<AsteroidTemplateDef>,
    pub elements: Vec<ElementDef>,
    pub module_defs: Vec<ModuleDef>,
    pub constants: Constants,
}
```

Replace the `ElementDef` struct with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDef {
    pub id: ElementId,
    pub density_kg_per_m3: f32,
    pub display_name: String,
    #[serde(default)]
    pub refined_name: Option<String>,
}
```

**Step 5: Add new `Command` and `Event` variants**

In the `Command` enum, add after `AssignShipTask`:

```rust
InstallModule {
    station_id: StationId,
    module_item_id: ModuleItemId,
},
UninstallModule {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
SetModuleEnabled {
    station_id: StationId,
    module_id: ModuleInstanceId,
    enabled: bool,
},
SetModuleThreshold {
    station_id: StationId,
    module_id: ModuleInstanceId,
    threshold_kg: f32,
},
```

In the `Event` enum, add after `OreDeposited`:

```rust
ModuleInstalled {
    station_id: StationId,
    module_id: ModuleInstanceId,
    module_item_id: ModuleItemId,
    module_def_id: String,
},
ModuleUninstalled {
    station_id: StationId,
    module_id: ModuleInstanceId,
    module_item_id: ModuleItemId,
},
ModuleToggled {
    station_id: StationId,
    module_id: ModuleInstanceId,
    enabled: bool,
},
ModuleThresholdSet {
    station_id: StationId,
    module_id: ModuleInstanceId,
    threshold_kg: f32,
},
RefineryRan {
    station_id: StationId,
    module_id: ModuleInstanceId,
    ore_consumed_kg: f32,
    material_produced_kg: f32,
    material_quality: f32,
    slag_produced_kg: f32,
},
```

**Step 6: Fix all struct literals broken by new fields**

`Counters` now requires two new fields. Update every `Counters { ... }` literal:

In `crates/sim_core/src/lib.rs` — `test_state()` function, update `counters`:
```rust
counters: Counters {
    next_event_id: 0,
    next_command_id: 0,
    next_asteroid_id: 0,
    next_lot_id: 0,
    next_module_instance_id: 0,
},
```

Also update the inline `Counters` in `transit_moves_ship_and_starts_next_task` test (around line 1289):
```rust
counters: Counters {
    next_event_id: 0,
    next_command_id: 0,
    next_asteroid_id: 0,
    next_lot_id: 0,
    next_module_instance_id: 0,
},
```

`GameContent` now requires `module_defs`. Update `test_content()` in `crates/sim_core/src/lib.rs` — add `module_defs: vec![]` to the `GameContent { ... }` literal.

`ElementDef` now has `refined_name: Option<String>`. Update every `ElementDef { ... }` literal in `test_content()`:
```rust
ElementDef {
    id: "ore".to_string(),
    density_kg_per_m3: 3000.0,
    display_name: "Raw Ore".to_string(),
    refined_name: None,
},
ElementDef {
    id: "Fe".to_string(),
    density_kg_per_m3: 7874.0,
    display_name: "Iron".to_string(),
    refined_name: Some("Iron Ingot".to_string()),
},
ElementDef {
    id: "Si".to_string(),
    density_kg_per_m3: 2329.0,
    display_name: "Silicon".to_string(),
    refined_name: None,
},
```

In `crates/sim_control/src/lib.rs` — `autopilot_content()` function:
- Add `module_defs: vec![]` to `GameContent { ... }`
- Add `refined_name: None` to the `ElementDef { id: "Fe", ... }` literal
- Update `Counters` in `autopilot_state()` to add `next_lot_id: 0, next_module_instance_id: 0`

**Step 7: Update `lib.rs` export**

In `crates/sim_core/src/lib.rs` line 13, the export will need updating after Task 2 renames the function. For now, leave it as is — just verify compile passes.

**Step 8: Verify**

Run: `cargo test`
Expected: All existing tests pass. No new tests yet.

**Step 9: Commit**

```bash
git add crates/sim_core/src/types.rs crates/sim_core/src/lib.rs crates/sim_control/src/lib.rs
git commit -m "feat: add inventory, module, and recipe types to sim_core"
```

---

### Task 2: Migrate `cargo` → `inventory` on ships and stations

**Files:**
- Modify: `crates/sim_core/src/types.rs` (ShipState, StationState)
- Modify: `crates/sim_core/src/tasks.rs` (rename fn, update resolve_mine/deposit)
- Modify: `crates/sim_core/src/lib.rs` (export, test helpers, cargo-related tests)
- Modify: `crates/sim_control/src/lib.rs` (cargo checks, test helpers)
- Modify: `crates/sim_cli/src/main.rs` (build_initial_state)

**Context:** This is the big breaking change. All references to `ship.cargo` and `station.cargo` are replaced. The `cargo_volume_used` function is renamed `inventory_volume_m3` with a new signature. `resolve_mine` now creates `InventoryItem::Ore`; `resolve_deposit` moves items.

**Step 1: Write failing tests for the new inventory shape**

In `crates/sim_core/src/lib.rs`, in the `// --- Cargo holds ---` section, add two new tests after the existing ones:

```rust
#[test]
fn test_mine_adds_ore_item_to_ship_inventory() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    // Give asteroid a known composition via deep scan unlock
    state.research.unlocked.insert(TechId("tech_deep_scan_v1".to_string()));
    let asteroid = state.asteroids.get_mut(&asteroid_id).unwrap();
    asteroid.knowledge.composition = Some(asteroid.true_composition.clone());

    let ship_id = ShipId("ship_0001".to_string());
    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 10;
    while state.meta.tick <= completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let has_ore = state.ships[&ship_id].inventory.iter().any(|i| {
        matches!(i, InventoryItem::Ore { asteroid_id: aid, .. } if aid == &asteroid_id)
    });
    assert!(has_ore, "ship inventory should contain an Ore item from the mined asteroid");
}

#[test]
fn test_deposit_moves_ore_items_to_station() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    let asteroid_id = AsteroidId("asteroid_test".to_string());
    state.ships.get_mut(&ship_id).unwrap().inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: asteroid_id.clone(),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
    });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station_has_ore = state.stations[&station_id].inventory.iter().any(|i| {
        matches!(i, InventoryItem::Ore { kg, .. } if *kg > 0.0)
    });
    assert!(station_has_ore, "station should have ore after deposit");
    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship inventory should be empty after deposit"
    );
}
```

Run: `cargo test test_mine_adds_ore_item test_deposit_moves_ore_items`
Expected: FAIL — `inventory` field doesn't exist yet on ShipState.

**Step 2: Update `ShipState` and `StationState` in `types.rs`**

Replace `ShipState`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipState {
    pub id: ShipId,
    pub location_node: NodeId,
    pub owner: PrincipalId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub task: Option<TaskState>,
}
```

Replace `StationState`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationState {
    pub id: StationId,
    pub location_node: NodeId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub power_available_per_tick: f32,
    pub facilities: FacilitiesState,
    pub modules: Vec<ModuleState>,
}
```

**Step 3: Replace `cargo_volume_used` with `inventory_volume_m3` in `tasks.rs`**

Replace the entire `cargo_volume_used` function with:

```rust
pub fn inventory_volume_m3(inventory: &[InventoryItem], content: &GameContent) -> f32 {
    inventory
        .iter()
        .map(|item| match item {
            InventoryItem::Ore { kg, .. } => {
                let density = content
                    .elements
                    .iter()
                    .find(|e| e.id == "ore")
                    .map(|e| e.density_kg_per_m3)
                    .unwrap_or(3000.0);
                kg / density
            }
            InventoryItem::Slag { kg, .. } => {
                let density = content
                    .elements
                    .iter()
                    .find(|e| e.id == "slag")
                    .map(|e| e.density_kg_per_m3)
                    .unwrap_or(2500.0);
                kg / density
            }
            InventoryItem::Material { element, kg, .. } => {
                let density = content
                    .elements
                    .iter()
                    .find(|e| e.id == *element)
                    .map(|e| e.density_kg_per_m3)
                    .unwrap_or(1000.0);
                kg / density
            }
            InventoryItem::Component { .. } => 0.0,
            InventoryItem::Module { module_def_id, .. } => content
                .module_defs
                .iter()
                .find(|m| m.id == *module_def_id)
                .map(|m| m.volume_m3)
                .unwrap_or(0.0),
        })
        .sum()
}
```

Update `mine_duration` to use `inventory_volume_m3`:
```rust
pub fn mine_duration(asteroid: &AsteroidState, ship: &ShipState, content: &GameContent) -> u64 {
    let ore_density = content
        .elements
        .iter()
        .find(|e| e.id == "ore")
        .map(|e| e.density_kg_per_m3)
        .unwrap_or(3000.0);
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = inventory_volume_m3(&ship.inventory, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);
    let rate = content.constants.mining_rate_kg_per_tick;

    let ticks_to_fill = (free_volume / (rate * effective_m3_per_kg)).ceil() as u64;
    let ticks_to_deplete = (asteroid.mass_kg / rate).ceil() as u64;

    ticks_to_fill.min(ticks_to_deplete).max(1)
}
```

**Step 4: Update `resolve_mine` in `tasks.rs`**

Replace `resolve_mine` with:

```rust
pub(crate) fn resolve_mine(
    state: &mut GameState,
    ship_id: &ShipId,
    asteroid_id: &AsteroidId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(asteroid) = state.asteroids.get(asteroid_id) else {
        set_ship_idle(state, ship_id, current_tick);
        return;
    };

    let ship = match state.ships.get(ship_id) {
        Some(s) => s,
        None => return,
    };

    let ore_density = content
        .elements
        .iter()
        .find(|e| e.id == "ore")
        .map(|e| e.density_kg_per_m3)
        .unwrap_or(3000.0);
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = inventory_volume_m3(&ship.inventory, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);
    let max_kg_by_volume = free_volume / effective_m3_per_kg;
    let extracted_total_kg = asteroid.mass_kg.min(max_kg_by_volume);

    // Snapshot composition at mine-time (use knowledge if available, else true composition).
    let composition = asteroid
        .knowledge
        .composition
        .clone()
        .unwrap_or_else(|| asteroid.true_composition.clone());

    let lot_id = LotId(format!("lot_{:04}", state.counters.next_lot_id));
    state.counters.next_lot_id += 1;

    let asteroid_remaining_kg = asteroid.mass_kg - extracted_total_kg;
    if asteroid_remaining_kg <= 0.0 {
        state.asteroids.remove(asteroid_id);
    } else if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.mass_kg = asteroid_remaining_kg;
    }

    let ore_item = InventoryItem::Ore {
        lot_id: lot_id.clone(),
        asteroid_id: asteroid_id.clone(),
        kg: extracted_total_kg,
        composition,
    };

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.inventory.push(ore_item.clone());
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreMined {
            ship_id: ship_id.clone(),
            asteroid_id: asteroid_id.clone(),
            ore_lot: ore_item,
            asteroid_remaining_kg: asteroid_remaining_kg.max(0.0),
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Mine".to_string(),
            target: Some(asteroid_id.0.clone()),
        },
    ));
}
```

**Step 5: Update `OreMined` and `OreDeposited` events in `types.rs`**

Replace the `OreMined` variant:
```rust
OreMined {
    ship_id: ShipId,
    asteroid_id: AsteroidId,
    ore_lot: InventoryItem,
    asteroid_remaining_kg: f32,
},
```

Replace the `OreDeposited` variant:
```rust
OreDeposited {
    ship_id: ShipId,
    station_id: StationId,
    items: Vec<InventoryItem>,
},
```

**Step 6: Update `resolve_deposit` in `tasks.rs`**

Replace `resolve_deposit` with:

```rust
pub(crate) fn resolve_deposit(
    state: &mut GameState,
    ship_id: &ShipId,
    station_id: &StationId,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(ship) = state.ships.get_mut(ship_id) else { return };
    let items: Vec<InventoryItem> = std::mem::take(&mut ship.inventory);

    if items.is_empty() {
        set_ship_idle(state, ship_id, current_tick);
        return;
    }

    let Some(station) = state.stations.get_mut(station_id) else {
        // Station gone — put items back and idle.
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.inventory = items;
        }
        set_ship_idle(state, ship_id, current_tick);
        return;
    };

    station.inventory.extend(items.clone());

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreDeposited {
            ship_id: ship_id.clone(),
            station_id: station_id.clone(),
            items,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Deposit".to_string(),
            target: Some(station_id.0.clone()),
        },
    ));
}
```

**Step 7: Update `lib.rs` export**

In `crates/sim_core/src/lib.rs` line 13, replace:
```rust
pub use tasks::{cargo_volume_used, mine_duration};
```
with:
```rust
pub use tasks::{inventory_volume_m3, mine_duration};
```

**Step 8: Update all test helpers and existing cargo-related tests in `lib.rs`**

In `test_state()`, update both `ShipState` and `StationState`:
```rust
ShipState {
    id: ship_id,
    location_node: node_id.clone(),
    owner,
    inventory: vec![],
    cargo_capacity_m3: 20.0,
    task: None,
},
```
```rust
StationState {
    id: station_id,
    location_node: node_id,
    inventory: vec![],
    cargo_capacity_m3: 10_000.0,
    power_available_per_tick: 100.0,
    facilities: FacilitiesState {
        compute_units_total: 10,
        power_per_compute_unit_per_tick: 1.0,
        efficiency: 1.0,
    },
    modules: vec![],
},
```

Update the inline `GameState` in `transit_moves_ship_and_starts_next_task` — replace `cargo: HashMap::new()` with `inventory: vec![]` on both `ShipState` and `StationState`, and add `modules: vec![]` to `StationState`.

Update `test_mine_adds_ore_to_ship_cargo` → rename to `test_mine_adds_ore_to_ship_inventory` and change the assertions:
```rust
let ship = &state.ships[&ship_id];
assert!(!ship.inventory.is_empty(), "ship inventory should not be empty after mining");
assert!(
    ship.inventory.iter().any(|i| matches!(i, InventoryItem::Ore { kg, .. } if *kg > 0.0)),
    "extracted mass must be positive"
);
```

Update `test_deposit_moves_cargo_to_station` → rename to `test_deposit_moves_inventory_to_station`. Replace `cargo.insert("Fe".to_string(), 100.0)` with:
```rust
state.ships.get_mut(&ship_id).unwrap().inventory.push(InventoryItem::Ore {
    lot_id: LotId("lot_test_0001".to_string()),
    asteroid_id: AsteroidId("asteroid_test".to_string()),
    kg: 100.0,
    composition: std::collections::HashMap::from([("Fe".to_string(), 0.7f32), ("Si".to_string(), 0.3f32)]),
});
```
Replace the assertion with:
```rust
let station_has_ore = state.stations[&station_id].inventory.iter().any(|i| {
    matches!(i, InventoryItem::Ore { kg, .. } if *kg > 90.0)
});
assert!(station_has_ore, "ore should transfer to station");
```

Update `test_deposit_clears_ship_cargo` → rename to `test_deposit_clears_ship_inventory`. Same `inventory.push` setup. Change assertion:
```rust
assert!(state.ships[&ship_id].inventory.is_empty(), "ship inventory should be empty after deposit");
```

Update `test_deposit_emits_ore_deposited_event` → same `inventory.push` setup for the ship.

Update `test_ship_starts_with_empty_cargo` → rename to `test_ship_starts_with_empty_inventory`:
```rust
assert!(ship.inventory.is_empty(), "ship inventory should be empty at start");
```

Update `test_station_starts_with_empty_cargo` → rename to `test_station_starts_with_empty_inventory`:
```rust
assert!(station.inventory.is_empty(), "station inventory should be empty at start");
```

**Step 9: Update `sim_control/src/lib.rs`**

Update import: add `InventoryItem, LotId` to the `use sim_core::{ ... }` list.

Replace the cargo check in `generate_commands` (around line 103):
```rust
if ship.inventory.iter().any(|i| matches!(i, InventoryItem::Ore { .. })) {
```

Update `autopilot_state()` — `ShipState` and `StationState`:
```rust
ShipState {
    id: ship_id,
    location_node: node.clone(),
    owner,
    inventory: vec![],
    cargo_capacity_m3: content.constants.ship_cargo_capacity_m3,
    task: None,
},
```
```rust
StationState {
    id: station_id,
    location_node: node,
    power_available_per_tick: 0.0,
    inventory: vec![],
    cargo_capacity_m3: content.constants.station_cargo_capacity_m3,
    facilities: FacilitiesState { ... },
    modules: vec![],
},
```

Update `test_autopilot_assigns_deposit_when_ship_has_cargo` — replace `cargo.insert(...)` with:
```rust
use sim_core::{AsteroidId, InventoryItem, LotId};
state.ships.get_mut(&ship_id).unwrap().inventory.push(InventoryItem::Ore {
    lot_id: LotId("lot_test_0001".to_string()),
    asteroid_id: AsteroidId("asteroid_test".to_string()),
    kg: 100.0,
    composition: std::collections::HashMap::from([("Fe".to_string(), 1.0f32)]),
});
```

**Step 10: Update `sim_cli/src/main.rs`**

In `build_initial_state`, replace `cargo: std::collections::HashMap::new()` with `inventory: vec![]` on both `ShipState` and `StationState`. Add `modules: vec![]` to `StationState`. Update `Counters` to add `next_lot_id: 0, next_module_instance_id: 0`.

Also update `print_status` — the existing code reads `station.cargo`, which no longer exists. Replace the cargo-related output with a simple inventory summary or remove it temporarily.

Update `load_content` to add `module_defs: vec![]` (placeholder until Task 4):
```rust
Ok(GameContent {
    content_version: techs_file.content_version,
    techs: techs_file.techs,
    solar_system,
    asteroid_templates: templates_file.templates,
    elements: elements_file.elements,
    module_defs: vec![],
    constants,
})
```

Also remove the unused `cargo_volume_used` import if it was imported anywhere.

**Step 11: Run tests**

Run: `cargo test`
Expected: All tests pass. The two new inventory tests added in Step 1 should now also pass.

**Step 12: Commit**

```bash
git add crates/sim_core/src/types.rs crates/sim_core/src/tasks.rs crates/sim_core/src/lib.rs crates/sim_control/src/lib.rs crates/sim_cli/src/main.rs
git commit -m "feat: migrate ship/station cargo to Vec<InventoryItem>"
```

---

### Task 3: Refinery tick logic and command resolution

**Files:**
- Create: `crates/sim_core/src/station.rs`
- Modify: `crates/sim_core/src/engine.rs`
- Modify: `crates/sim_core/src/lib.rs` (mod declaration + tests)

**Context:** Processors run every `processing_interval_ticks` ticks if enabled and above threshold. Command resolution handles `InstallModule`, `UninstallModule`, `SetModuleEnabled`, `SetModuleThreshold`.

**Step 1: Write failing tests**

In `crates/sim_core/src/lib.rs`, add a new test section `// --- Refinery ---`:

```rust
// --- Refinery ---

fn refinery_content() -> GameContent {
    let mut content = test_content();
    content.module_defs = vec![ModuleDef {
        id: "module_basic_iron_refinery".to_string(),
        name: "Basic Iron Refinery".to_string(),
        mass_kg: 5000.0,
        volume_m3: 10.0,
        power_consumption_per_run: 10.0,
        behavior: ModuleBehaviorDef::Processor(ProcessorDef {
            processing_interval_ticks: 2, // short for tests
            recipes: vec![RecipeDef {
                id: "recipe_basic_iron".to_string(),
                inputs: vec![RecipeInput {
                    filter: InputFilter::ItemKind(ItemKind::Ore),
                    amount: InputAmount::Kg(500.0),
                }],
                outputs: vec![
                    OutputSpec::Material {
                        element: "Fe".to_string(),
                        yield_formula: YieldFormula::ElementFraction { element: "Fe".to_string() },
                        quality_formula: QualityFormula::ElementFractionTimesMultiplier {
                            element: "Fe".to_string(),
                            multiplier: 1.0,
                        },
                    },
                    OutputSpec::Slag {
                        yield_formula: YieldFormula::FixedFraction(1.0),
                    },
                ],
                efficiency: 1.0,
            }],
        }),
    }];
    content
}

fn state_with_refinery(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Pre-install the module (skip the install command path for simplicity)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
        }),
    });

    // Seed station with ore (70% Fe, 30% Si)
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: AsteroidId("asteroid_0001".to_string()),
        kg: 1000.0,
        composition: std::collections::HashMap::from([
            ("Fe".to_string(), 0.7f32),
            ("Si".to_string(), 0.3f32),
        ]),
    });

    state
}

#[test]
fn test_refinery_produces_material_and_slag() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    // Run enough ticks for the refinery to fire (interval=2)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let has_material = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Material { element, kg, .. } if element == "Fe" && *kg > 0.0)
    });
    assert!(has_material, "station should have Fe Material after refinery runs");

    let has_slag = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Slag { kg, .. } if *kg > 0.0)
    });
    assert!(has_slag, "station should have Slag after refinery runs");
}

#[test]
fn test_refinery_quality_equals_fe_fraction() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let quality = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Material { element, quality, .. } = i {
            if element == "Fe" { Some(*quality) } else { None }
        } else { None }
    });
    assert!(quality.is_some(), "Fe Material should exist");
    assert!(
        (quality.unwrap() - 0.7).abs() < 1e-4,
        "quality should equal Fe fraction (0.7) with multiplier 1.0"
    );
}

#[test]
fn test_refinery_skips_when_below_threshold() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set threshold above available ore
    if let Some(ModuleKindState::Processor(ps)) = state
        .stations.get_mut(&station_id).unwrap()
        .modules[0].kind_state.as_processor_mut()
    {
        ps.threshold_kg = 9999.0;
    }

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let has_material = station.inventory.iter().any(|i| matches!(i, InventoryItem::Material { .. }));
    assert!(!has_material, "refinery should not run when ore is below threshold");
}

#[test]
fn test_refinery_emits_refinery_ran_event() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events.iter().any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "RefineryRan event should be emitted when refinery processes ore"
    );
}
```

Note: the `as_processor_mut()` method doesn't exist yet — the test will fail to compile until you add it or restructure the threshold test to use a direct state mutation via pattern matching. Simplest: replace that test's threshold setup with constructing a fresh state with threshold directly set (no helper method needed):

```rust
#[test]
fn test_refinery_skips_when_below_threshold() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 9999.0,  // higher than available ore
            ticks_since_last_run: 0,
        }),
    });
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: AsteroidId("asteroid_0001".to_string()),
        kg: 1000.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 0.7f32), ("Si".to_string(), 0.3f32)]),
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        !station.inventory.iter().any(|i| matches!(i, InventoryItem::Material { .. })),
        "refinery should not run when ore is below threshold"
    );
}
```

Run: `cargo test test_refinery`
Expected: FAIL — `tick_stations` not called yet.

**Step 2: Create `crates/sim_core/src/station.rs`**

Create the file with this content:

```rust
use crate::{
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ItemKind, ModuleBehaviorDef, ModuleKindState, OutputSpec, QualityFormula, StationId,
    YieldFormula,
};
use std::collections::HashMap;

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();

    for station_id in station_ids {
        tick_station_modules(state, &station_id, content, events);
    }
}

fn tick_station_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let module_count = state
        .stations
        .get(station_id)
        .map(|s| s.modules.len())
        .unwrap_or(0);

    for module_idx in 0..module_count {
        let (def_id, enabled, interval, power_needed) = {
            let station = match state.stations.get(station_id) {
                Some(s) => s,
                None => return,
            };
            let module = &station.modules[module_idx];
            if !module.enabled {
                continue;
            }
            let def = match content.module_defs.iter().find(|d| d.id == module.def_id) {
                Some(d) => d,
                None => continue,
            };
            let interval = match &def.behavior {
                ModuleBehaviorDef::Processor(p) => p.processing_interval_ticks,
                ModuleBehaviorDef::Storage { .. } => continue,
            };
            (module.def_id.clone(), module.enabled, interval, def.power_consumption_per_run)
        };
        let _ = (def_id, enabled);

        // Increment timer
        {
            let station = match state.stations.get_mut(station_id) {
                Some(s) => s,
                None => return,
            };
            if let ModuleKindState::Processor(ps) = &mut station.modules[module_idx].kind_state {
                ps.ticks_since_last_run += 1;
                if ps.ticks_since_last_run < interval {
                    continue;
                }
            }
        }

        // Check power
        {
            let station = match state.stations.get(station_id) {
                Some(s) => s,
                None => return,
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Check threshold
        let threshold_kg = {
            let station = match state.stations.get(station_id) {
                Some(s) => s,
                None => return,
            };
            if let ModuleKindState::Processor(ps) = &station.modules[module_idx].kind_state {
                ps.threshold_kg
            } else {
                continue;
            }
        };

        let total_ore_kg: f32 = state
            .stations
            .get(station_id)
            .map(|s| {
                s.inventory
                    .iter()
                    .filter_map(|i| if let InventoryItem::Ore { kg, .. } = i { Some(*kg) } else { None })
                    .sum()
            })
            .unwrap_or(0.0);

        if total_ore_kg < threshold_kg {
            continue;
        }

        // Run the recipe
        resolve_processor_run(state, station_id, module_idx, content, events);

        // Reset timer
        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Processor(ps) = &mut station.modules[module_idx].kind_state {
                ps.ticks_since_last_run = 0;
            }
        }
    }
}

fn resolve_processor_run(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let (module_id, def_id) = {
        let station = match state.stations.get(station_id) {
            Some(s) => s,
            None => return,
        };
        let module = &station.modules[module_idx];
        (module.id.clone(), module.def_id.clone())
    };

    let def = match content.module_defs.iter().find(|d| d.id == def_id) {
        Some(d) => d,
        None => return,
    };

    let processor_def = match &def.behavior {
        ModuleBehaviorDef::Processor(p) => p,
        _ => return,
    };

    let recipe = match processor_def.recipes.first() {
        Some(r) => r,
        None => return,
    };

    // Determine how much ore to consume.
    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return,
    };

    let matches_ore = |i: &InventoryItem| -> bool {
        match &recipe.inputs.first().map(|inp| &inp.filter) {
            Some(InputFilter::ItemKind(ItemKind::Ore)) => matches!(i, InventoryItem::Ore { .. }),
            Some(InputFilter::ItemKind(ItemKind::Material)) => matches!(i, InventoryItem::Material { .. }),
            _ => false,
        }
    };

    // Consume up to rate_kg from matching items FIFO.
    let mut remaining_to_consume = rate_kg;
    let mut consumed_kg = 0.0f32;
    // Weighted composition across consumed lots.
    let mut weighted_composition: HashMap<String, f32> = HashMap::new();
    let mut weighted_total_kg = 0.0f32;

    if let Some(station) = state.stations.get_mut(station_id) {
        let mut new_inventory: Vec<InventoryItem> = Vec::new();
        for item in station.inventory.drain(..) {
            if remaining_to_consume > 0.0 && matches_ore(&item) {
                if let InventoryItem::Ore { kg, composition, .. } = &item {
                    let take = kg.min(remaining_to_consume);
                    remaining_to_consume -= take;
                    consumed_kg += take;
                    // Accumulate weighted composition
                    for (element, fraction) in composition {
                        *weighted_composition.entry(element.clone()).or_insert(0.0) +=
                            fraction * take;
                    }
                    weighted_total_kg += take;
                    if *kg - take > 1e-3 {
                        // Partial consumption: keep remainder
                        new_inventory.push(InventoryItem::Ore {
                            lot_id: if let InventoryItem::Ore { lot_id, .. } = &item { lot_id.clone() } else { unreachable!() },
                            asteroid_id: if let InventoryItem::Ore { asteroid_id, .. } = &item { asteroid_id.clone() } else { unreachable!() },
                            kg: kg - take,
                            composition: if let InventoryItem::Ore { composition, .. } = &item { composition.clone() } else { unreachable!() },
                        });
                    }
                } else {
                    new_inventory.push(item);
                }
            } else {
                new_inventory.push(item);
            }
        }
        station.inventory = new_inventory;
    }

    if consumed_kg < 1e-3 {
        return;
    }

    // Normalize weighted composition to fractions.
    let avg_composition: HashMap<String, f32> = if weighted_total_kg > 0.0 {
        weighted_composition
            .iter()
            .map(|(k, v)| (k.clone(), v / weighted_total_kg))
            .collect()
    } else {
        HashMap::new()
    };

    // Compute outputs.
    let mut material_kg = 0.0f32;
    let mut material_quality = 0.0f32;
    let mut slag_kg = 0.0f32;
    let mut slag_composition: HashMap<String, f32> = HashMap::new();

    for output in &recipe.outputs {
        match output {
            OutputSpec::Material { element, yield_formula, quality_formula } => {
                let yield_frac = match yield_formula {
                    YieldFormula::ElementFraction { element: el } => {
                        avg_composition.get(el).copied().unwrap_or(0.0)
                    }
                    YieldFormula::FixedFraction(f) => *f,
                };
                material_kg = consumed_kg * yield_frac;
                material_quality = match quality_formula {
                    QualityFormula::ElementFractionTimesMultiplier { element: el, multiplier } => {
                        (avg_composition.get(el).copied().unwrap_or(0.0) * multiplier).clamp(0.0, 1.0)
                    }
                    QualityFormula::Fixed(q) => *q,
                };
                if material_kg > 1e-3 {
                    if let Some(station) = state.stations.get_mut(station_id) {
                        station.inventory.push(InventoryItem::Material {
                            element: element.clone(),
                            kg: material_kg,
                            quality: material_quality,
                        });
                    }
                }
            }
            OutputSpec::Slag { yield_formula } => {
                let yield_frac = match yield_formula {
                    YieldFormula::FixedFraction(f) => *f,
                    YieldFormula::ElementFraction { element } => {
                        avg_composition.get(element).copied().unwrap_or(0.0)
                    }
                };
                // Slag = consumed - material (remainder after extraction)
                slag_kg = (consumed_kg - material_kg) * yield_frac;
                // Slag composition = non-extracted element fractions, normalized
                let non_material_total: f32 = avg_composition
                    .iter()
                    .filter(|(k, _)| *k != &recipe.outputs.iter().find_map(|o| {
                        if let OutputSpec::Material { element, .. } = o { Some(element.clone()) } else { None }
                    }).unwrap_or_default())
                    .map(|(_, v)| v)
                    .sum();
                if non_material_total > 1e-6 {
                    for (el, frac) in &avg_composition {
                        let is_extracted = recipe.outputs.iter().any(|o| {
                            if let OutputSpec::Material { element, .. } = o { element == el } else { false }
                        });
                        if !is_extracted {
                            slag_composition.insert(el.clone(), frac / non_material_total);
                        }
                    }
                }

                if slag_kg > 1e-3 {
                    if let Some(station) = state.stations.get_mut(station_id) {
                        // Find existing Slag item and blend, or insert new.
                        let existing_slag = station.inventory.iter_mut().find(|i| matches!(i, InventoryItem::Slag { .. }));
                        if let Some(InventoryItem::Slag { kg: existing_kg, composition: existing_comp }) = existing_slag {
                            let total = *existing_kg + slag_kg;
                            let blended: HashMap<String, f32> = {
                                let mut keys: std::collections::HashSet<String> = existing_comp.keys().cloned().collect();
                                keys.extend(slag_composition.keys().cloned());
                                keys.into_iter()
                                    .map(|k| {
                                        let a = existing_comp.get(&k).copied().unwrap_or(0.0) * *existing_kg;
                                        let b = slag_composition.get(&k).copied().unwrap_or(0.0) * slag_kg;
                                        (k, (a + b) / total)
                                    })
                                    .collect()
                            };
                            *existing_kg = total;
                            *existing_comp = blended;
                        } else {
                            station.inventory.push(InventoryItem::Slag {
                                kg: slag_kg,
                                composition: slag_composition.clone(),
                            });
                        }
                    }
                }
            }
            OutputSpec::Component { .. } => {} // future
        }
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::RefineryRan {
            station_id: station_id.clone(),
            module_id,
            ore_consumed_kg: consumed_kg,
            material_produced_kg: material_kg,
            material_quality,
            slag_produced_kg: slag_kg,
        },
    ));
}
```

Note: the partial consumption code above has a structural issue with the `drain` + `item` pattern. Simplify by collecting indices or cloning. Here's a cleaner version for the FIFO consumption loop:

```rust
// Consume up to rate_kg from Ore items (FIFO by insertion order).
if let Some(station) = state.stations.get_mut(station_id) {
    let mut remaining = rate_kg;
    let mut new_inventory: Vec<InventoryItem> = Vec::new();

    for item in station.inventory.drain(..) {
        if remaining > 0.0 {
            if let InventoryItem::Ore { lot_id, asteroid_id, kg, composition } = &item {
                let take = kg.min(remaining);
                remaining -= take;
                consumed_kg += take;
                for (element, fraction) in composition {
                    *weighted_composition.entry(element.clone()).or_insert(0.0) += fraction * take;
                }
                weighted_total_kg += take;
                let leftover = kg - take;
                if leftover > 1e-3 {
                    new_inventory.push(InventoryItem::Ore {
                        lot_id: lot_id.clone(),
                        asteroid_id: asteroid_id.clone(),
                        kg: leftover,
                        composition: composition.clone(),
                    });
                }
                continue;
            }
        }
        new_inventory.push(item);
    }
    station.inventory = new_inventory;
}
```

**Step 3: Declare the station module in `lib.rs`**

In `crates/sim_core/src/lib.rs`, add `mod station;` after `mod tasks;`:
```rust
mod station;
```

**Step 4: Call `tick_stations` in `engine.rs`**

Add import at top of `engine.rs`:
```rust
use crate::station::tick_stations;
```

In the `tick` function, call `tick_stations` after `resolve_ship_tasks`:
```rust
apply_commands(state, commands, content, &mut events);
resolve_ship_tasks(state, content, rng, &mut events);
tick_stations(state, content, &mut events);
advance_research(state, content, rng, event_level, &mut events);
```

**Step 5: Handle new commands in `engine.rs`**

In `apply_commands`, extend the `match &envelope.command` block. Add cases after `AssignShipTask`:

```rust
Command::InstallModule { station_id, module_item_id } => {
    let Some(station) = state.stations.get_mut(station_id) else { continue };
    let item_pos = station.inventory.iter().position(|i| {
        matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id)
    });
    let Some(pos) = item_pos else { continue };
    let InventoryItem::Module { item_id, module_def_id } = station.inventory.remove(pos) else { continue };

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);

    // Determine kind_state from def
    let kind_state = match content.module_defs.iter().find(|d| d.id == module_def_id) {
        Some(def) => match &def.behavior {
            crate::ModuleBehaviorDef::Processor(_) => crate::ModuleKindState::Processor(crate::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
            }),
            crate::ModuleBehaviorDef::Storage { .. } => crate::ModuleKindState::Storage,
        },
        None => continue,
    };

    station.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: false,
        kind_state,
    });

    events.push(crate::emit(&mut state.counters, current_tick, Event::ModuleInstalled {
        station_id: station_id.clone(),
        module_id,
        module_item_id: item_id,
        module_def_id,
    }));
}
Command::UninstallModule { station_id, module_id } => {
    let Some(station) = state.stations.get_mut(station_id) else { continue };
    let pos = station.modules.iter().position(|m| &m.id == module_id);
    let Some(pos) = pos else { continue };
    let module = station.modules.remove(pos);

    // Generate a new item_id for the returned item
    let item_id = crate::ModuleItemId(format!("module_item_{:04}", state.counters.next_module_instance_id));
    state.counters.next_module_instance_id += 1;

    station.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: module.def_id.clone(),
    });

    events.push(crate::emit(&mut state.counters, current_tick, Event::ModuleUninstalled {
        station_id: station_id.clone(),
        module_id: module_id.clone(),
        module_item_id: item_id,
    }));
}
Command::SetModuleEnabled { station_id, module_id, enabled } => {
    let Some(station) = state.stations.get_mut(station_id) else { continue };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else { continue };
    module.enabled = *enabled;
    events.push(crate::emit(&mut state.counters, current_tick, Event::ModuleToggled {
        station_id: station_id.clone(),
        module_id: module_id.clone(),
        enabled: *enabled,
    }));
}
Command::SetModuleThreshold { station_id, module_id, threshold_kg } => {
    let Some(station) = state.stations.get_mut(station_id) else { continue };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else { continue };
    if let crate::ModuleKindState::Processor(ps) = &mut module.kind_state {
        ps.threshold_kg = *threshold_kg;
    }
    events.push(crate::emit(&mut state.counters, current_tick, Event::ModuleThresholdSet {
        station_id: station_id.clone(),
        module_id: module_id.clone(),
        threshold_kg: *threshold_kg,
    }));
}
```

Note: the `apply_commands` function currently builds an `assignments` vec to avoid borrow issues. Restructure it to handle all command types — move from a pre-collection pattern to direct processing per command type, or keep ship assignments as a collected vec and handle module commands inline. The simplest approach: add module commands to a separate collected vec and process after ship assignments.

**Step 6: Run tests**

Run: `cargo test test_refinery`
Expected: All 4 refinery tests pass.

Run: `cargo test`
Expected: All tests pass.

**Step 7: Commit**

```bash
git add crates/sim_core/src/station.rs crates/sim_core/src/engine.rs crates/sim_core/src/lib.rs
git commit -m "feat: add refinery tick logic and module command resolution"
```

---

### Task 4: Content files and CLI module loading

**Files:**
- Modify: `content/elements.json`
- Create: `content/module_defs.json`
- Create: `content/dev_base_state.json`
- Modify: `crates/sim_cli/src/main.rs`

**Step 1: Update `content/elements.json`**

Replace the file contents with:

```json
{
  "elements": [
    { "id": "ore",  "density_kg_per_m3": 3000.0, "display_name": "Raw Ore",    "refined_name": null },
    { "id": "slag", "density_kg_per_m3": 2500.0, "display_name": "Slag",       "refined_name": null },
    { "id": "Fe",   "density_kg_per_m3": 7874.0, "display_name": "Iron",       "refined_name": "Iron Ingot" },
    { "id": "Si",   "density_kg_per_m3": 2329.0, "display_name": "Silicon",    "refined_name": null },
    { "id": "He",   "density_kg_per_m3": 125.0,  "display_name": "Helium-3",   "refined_name": "Liquid Helium-3" }
  ]
}
```

Note: He density changed from 0.164 (gas) to 125.0 (liquid helium density) since in the game context it's stored as liquid.

**Step 2: Create `content/module_defs.json`**

```json
[
  {
    "id": "module_basic_iron_refinery",
    "name": "Basic Iron Refinery",
    "mass_kg": 5000.0,
    "volume_m3": 10.0,
    "power_consumption_per_run": 10.0,
    "behavior": {
      "Processor": {
        "processing_interval_ticks": 60,
        "recipes": [
          {
            "id": "recipe_basic_iron",
            "inputs": [
              {
                "filter": { "ItemKind": "Ore" },
                "amount": { "Kg": 1000.0 }
              }
            ],
            "outputs": [
              {
                "Material": {
                  "element": "Fe",
                  "yield_formula": { "ElementFraction": { "element": "Fe" } },
                  "quality_formula": {
                    "ElementFractionTimesMultiplier": { "element": "Fe", "multiplier": 1.0 }
                  }
                }
              },
              {
                "Slag": {
                  "yield_formula": { "FixedFraction": 1.0 }
                }
              }
            ],
            "efficiency": 1.0
          }
        ]
      }
    }
  }
]
```

**Step 3: Update `sim_cli/src/main.rs` to load `module_defs.json`**

Add a new file struct:
```rust
#[derive(Deserialize)]
struct ModuleDefsFile {
    // module_defs.json is a top-level array, not wrapped in an object
}
```

Actually `module_defs.json` is a top-level JSON array, so deserialize directly:
```rust
let module_defs: Vec<sim_core::ModuleDef> = serde_json::from_str(
    &std::fs::read_to_string(dir.join("module_defs.json")).context("reading module_defs.json")?,
)
.context("parsing module_defs.json")?;
```

Add `sim_core::ModuleDef` to the imports at the top of `main.rs`.

Update `load_content` to return the loaded `module_defs`:
```rust
Ok(GameContent {
    content_version: techs_file.content_version,
    techs: techs_file.techs,
    solar_system,
    asteroid_templates: templates_file.templates,
    elements: elements_file.elements,
    module_defs,
    constants,
})
```

Also update `ElementsFile` to include `refined_name`. Since `ElementDef` has `#[serde(default)]` on `refined_name`, the existing `ElementsFile` will work without changes — `elements_file.elements` will deserialize correctly.

**Step 4: Create `content/dev_base_state.json`**

Check `content/solar_system.json` to get exact node IDs. The nodes are: `node_earth_orbit`, `node_belt_inner`, `node_belt_mid`, `node_belt_outer`.

Check `content/asteroid_templates.json` to get template IDs (for scan sites). Create the file:

```json
{
  "meta": {
    "tick": 0,
    "seed": 42,
    "schema_version": 1,
    "content_version": "0.1.0"
  },
  "scan_sites": [
    { "id": "site_0001", "node": "node_belt_inner",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0002", "node": "node_belt_inner",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0003", "node": "node_belt_mid",    "template_id": "tmpl_iron_rich" },
    { "id": "site_0004", "node": "node_belt_mid",    "template_id": "tmpl_iron_rich" },
    { "id": "site_0005", "node": "node_belt_outer",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0006", "node": "node_belt_outer",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0007", "node": "node_belt_inner",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0008", "node": "node_belt_mid",    "template_id": "tmpl_iron_rich" },
    { "id": "site_0009", "node": "node_belt_outer",  "template_id": "tmpl_iron_rich" },
    { "id": "site_0010", "node": "node_belt_inner",  "template_id": "tmpl_iron_rich" }
  ],
  "asteroids": {},
  "ships": {
    "ship_0001": {
      "id": "ship_0001",
      "location_node": "node_earth_orbit",
      "owner": "principal_autopilot",
      "inventory": [],
      "cargo_capacity_m3": 20.0,
      "task": null
    }
  },
  "stations": {
    "station_earth_orbit": {
      "id": "station_earth_orbit",
      "location_node": "node_earth_orbit",
      "inventory": [
        {
          "kind": "Module",
          "item_id": "module_item_0001",
          "module_def_id": "module_basic_iron_refinery"
        }
      ],
      "cargo_capacity_m3": 10000.0,
      "power_available_per_tick": 100.0,
      "facilities": {
        "compute_units_total": 10,
        "power_per_compute_unit_per_tick": 1.0,
        "efficiency": 1.0
      },
      "modules": []
    }
  },
  "research": {
    "unlocked": [],
    "data_pool": {},
    "evidence": {}
  },
  "counters": {
    "next_event_id": 0,
    "next_command_id": 0,
    "next_asteroid_id": 0,
    "next_lot_id": 0,
    "next_module_instance_id": 1
  }
}
```

Note: check `content/asteroid_templates.json` for the actual template ID — it may differ from `tmpl_iron_rich`. Also check `content/techs.json` for the `content_version` value to use.

**Step 5: Verify**

Run: `cargo build -p sim_cli`
Expected: Builds without errors.

Run: `cargo run -p sim_cli -- run --seed 42 --ticks 10`
Expected: Runs 10 ticks, prints status.

**Step 6: Commit**

```bash
git add content/elements.json content/module_defs.json content/dev_base_state.json crates/sim_cli/src/main.rs
git commit -m "feat: add module content, dev_base_state, and module loading to sim_cli"
```

---

### Task 5: CLI `--state` flag (save/load foundation)

**Files:**
- Modify: `crates/sim_cli/src/main.rs`

**Step 1: Add `--state` flag to the `Run` command**

The `Run` subcommand currently requires `--seed`. Make `--seed` optional and add `--state`:

```rust
#[derive(Subcommand)]
enum Commands {
    Run {
        #[arg(long)]
        ticks: u64,
        /// Mutually exclusive with --state: generate world procedurally with this seed.
        #[arg(long, conflicts_with = "state_file")]
        seed: Option<u64>,
        /// Mutually exclusive with --seed: load initial GameState from a JSON file.
        #[arg(long = "state", conflicts_with = "seed")]
        state_file: Option<String>,
        #[arg(long, default_value = "./content")]
        content_dir: String,
        #[arg(long, default_value_t = 100)]
        print_every: u64,
        #[arg(long, default_value = "normal", value_parser = ["normal", "debug"])]
        event_level: String,
    },
}
```

**Step 2: Update `run()` to branch on `seed` vs `state_file`**

```rust
fn run(
    ticks: u64,
    seed: Option<u64>,
    state_file: Option<String>,
    content_dir: &str,
    print_every: u64,
    event_level: EventLevel,
) -> Result<()> {
    let content = load_content(content_dir)?;

    let (mut state, mut rng) = if let Some(path) = state_file {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading state file: {path}"))?;
        let loaded: GameState = serde_json::from_str(&json)
            .with_context(|| format!("parsing state file: {path}"))?;
        let rng_seed = loaded.meta.seed;
        (loaded, ChaCha8Rng::seed_from_u64(rng_seed))
    } else {
        let s = seed.unwrap_or_else(|| rand::random());
        let mut rng = ChaCha8Rng::seed_from_u64(s);
        let state = build_initial_state(&content, s, &mut rng);
        (state, rng)
    };

    // ... rest of run loop unchanged
}
```

Update `main()` to pass both `seed` and `state_file` to `run`.

**Step 3: Add `rand` import for `rand::random()`**

`rand::random()` should already be available since `rand` is a dependency. If not, use `0u64` as the fallback seed.

**Step 4: Verify**

Run: `cargo run -p sim_cli -- run --state content/dev_base_state.json --ticks 50`
Expected: Loads state from file, runs 50 ticks.

Run: `cargo run -p sim_cli -- run --seed 42 --ticks 10`
Expected: Procedural gen still works.

**Step 5: Commit**

```bash
git add crates/sim_cli/src/main.rs
git commit -m "feat: add --state flag to sim_cli for loading saved game state"
```

---

### Task 6: Autopilot auto-install and module setup

**Files:**
- Modify: `crates/sim_control/src/lib.rs`

**Context:** On each tick, the autopilot checks each station for uninstalled `Module` items and issues `InstallModule`. After a `ModuleInstalled` event is observed (via state — the module appears in `station.modules`), the autopilot issues `SetModuleEnabled(true)` and `SetModuleThreshold(500.0)`. Since the autopilot is stateless, it detects "needs enabling" by checking for modules with `enabled: false`.

**Step 1: Write failing test**

In `crates/sim_control/src/lib.rs`, add:

```rust
#[test]
fn test_autopilot_installs_module_in_station_inventory() {
    let content = autopilot_content();
    let mut state = autopilot_state(&content);

    // Add a Module item to the station inventory
    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().inventory.push(
        sim_core::InventoryItem::Module {
            item_id: sim_core::ModuleItemId("module_item_0001".to_string()),
            module_def_id: "module_basic_iron_refinery".to_string(),
        }
    );

    let mut autopilot = AutopilotController;
    let mut next_id = 0u64;
    let commands = autopilot.generate_commands(&state, &content, &mut next_id);

    assert!(
        commands.iter().any(|cmd| matches!(
            &cmd.command,
            sim_core::Command::InstallModule { .. }
        )),
        "autopilot should issue InstallModule when Module item is in station inventory"
    );
}

#[test]
fn test_autopilot_enables_disabled_module() {
    let content = autopilot_content();
    let mut state = autopilot_state(&content);

    // Add an installed but disabled module
    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().modules.push(sim_core::ModuleState {
        id: sim_core::ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: false,
        kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
        }),
    });

    let mut autopilot = AutopilotController;
    let mut next_id = 0u64;
    let commands = autopilot.generate_commands(&state, &content, &mut next_id);

    assert!(
        commands.iter().any(|cmd| matches!(
            &cmd.command,
            sim_core::Command::SetModuleEnabled { enabled: true, .. }
        )),
        "autopilot should enable a disabled installed module"
    );
}
```

Run: `cargo test test_autopilot_installs test_autopilot_enables`
Expected: FAIL.

**Step 2: Add auto-install logic to `generate_commands`**

Add to imports at top of `sim_control/src/lib.rs`:
```rust
use sim_core::{
    Command, CommandEnvelope, CommandId, GameContent, GameState, InventoryItem,
    ModuleInstanceId, ModuleItemId, ModuleKindState, PrincipalId, SiteId, TaskKind, TechId,
    // ... existing imports
};
```

At the start of `generate_commands`, before the ship loop, add station module management:

```rust
// Station module management: install uninstalled modules, enable/configure installed ones.
for station in state.stations.values() {
    // Install any Module items sitting in station inventory.
    for item in &station.inventory {
        if let InventoryItem::Module { item_id, .. } = item {
            let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
            *next_command_id += 1;
            commands.push(CommandEnvelope {
                id: cmd_id,
                issued_by: owner.clone(),
                issued_tick: state.meta.tick,
                execute_at_tick: state.meta.tick,
                command: Command::InstallModule {
                    station_id: station.id.clone(),
                    module_item_id: item_id.clone(),
                },
            });
        }
    }

    // Enable disabled modules and set threshold.
    for module in &station.modules {
        if !module.enabled {
            let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
            *next_command_id += 1;
            commands.push(CommandEnvelope {
                id: cmd_id,
                issued_by: owner.clone(),
                issued_tick: state.meta.tick,
                execute_at_tick: state.meta.tick,
                command: Command::SetModuleEnabled {
                    station_id: station.id.clone(),
                    module_id: module.id.clone(),
                    enabled: true,
                },
            });
        }

        // Set threshold if it's still 0 (just installed).
        if let ModuleKindState::Processor(ps) = &module.kind_state {
            if ps.threshold_kg == 0.0 {
                let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
                *next_command_id += 1;
                commands.push(CommandEnvelope {
                    id: cmd_id,
                    issued_by: owner.clone(),
                    issued_tick: state.meta.tick,
                    execute_at_tick: state.meta.tick,
                    command: Command::SetModuleThreshold {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        threshold_kg: 500.0,
                    },
                });
            }
        }
    }
}
```

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add crates/sim_control/src/lib.rs
git commit -m "feat: autopilot auto-installs and enables station modules"
```

---

### Task 7: Frontend types

**Files:**
- Modify: `ui_web/src/types.ts`
- Modify: `ui_web/src/components/FleetPanel.test.tsx`

**Context:** `cargo` and `cargo_capacity_m3` still exist on the Rust types in name, but `cargo` is now `inventory`. The frontend types must match. `OreCompositions` is removed — composition is embedded in `Ore` items.

**Step 1: Write failing type-check test**

Run: `cd ui_web && npx tsc --noEmit`
Note the errors. Most will be about `cargo` not existing on ship/station types.

**Step 2: Replace `types.ts`**

Open `ui_web/src/types.ts`. Replace the `ShipState` and `StationState` interfaces and add `InventoryItem` types:

```typescript
// Inventory item discriminated union (matches Rust InventoryItem serde tag = "kind")
export type CompositionVec = Record<string, number>

export interface OreItem {
  kind: 'Ore'
  lot_id: string
  asteroid_id: string
  kg: number
  composition: CompositionVec
}

export interface SlagItem {
  kind: 'Slag'
  kg: number
  composition: CompositionVec
}

export interface MaterialItem {
  kind: 'Material'
  element: string
  kg: number
  quality: number
}

export interface ComponentItem {
  kind: 'Component'
  component_id: string
  count: number
  quality: number
}

export interface ModuleItem {
  kind: 'Module'
  item_id: string
  module_def_id: string
}

export type InventoryItem = OreItem | SlagItem | MaterialItem | ComponentItem | ModuleItem

// Module state
export interface ProcessorState {
  threshold_kg: number
  ticks_since_last_run: number
}

export type ModuleKindState =
  | { Processor: ProcessorState }
  | 'Storage'

export interface ModuleState {
  id: string
  def_id: string
  enabled: boolean
  kind_state: ModuleKindState
}
```

Update `ShipState`:
```typescript
export interface ShipState {
  id: string
  location_node: string
  owner: string
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  task: TaskState | null
}
```

Update `StationState`:
```typescript
export interface StationState {
  id: string
  location_node: string
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  power_available_per_tick: number
  facilities: FacilitiesState
  modules: ModuleState[]
}
```

Remove any `cargo: Record<string, number>` fields from both interfaces.

**Step 3: Update `FleetPanel.test.tsx`**

The mock ship uses `cargo: { Fe: 150.0, Si: 30.0 }` — replace with `inventory`:

```typescript
const mockShip: ShipState = {
  id: 'ship_0001',
  location_node: 'node_earth_orbit',
  owner: 'principal_autopilot',
  inventory: [
    {
      kind: 'Ore',
      lot_id: 'lot_0001',
      asteroid_id: 'asteroid_0001',
      kg: 180.0,
      composition: { Fe: 0.83, Si: 0.17 },
    },
  ],
  cargo_capacity_m3: 20.0,
  task: null,
}
```

Update the test assertions — `screen.getByText(/Fe/)` should still work if `FleetPanel` renders composition percentages. Update test 2 to check for ore rendering.

**Step 4: Type-check**

Run: `cd ui_web && npx tsc --noEmit`
Expected: No errors related to `cargo`.

**Step 5: Commit**

```bash
git add ui_web/src/types.ts ui_web/src/components/FleetPanel.test.tsx
git commit -m "feat: update frontend types for Vec<InventoryItem> inventory model"
```

---

### Task 8: Frontend stream and event handling

**Files:**
- Modify: `ui_web/src/hooks/useSimStream.ts`

**Context:** Remove `OreCompositions`. Update `applyEvents` for the new inventory model. Add handlers for `ModuleInstalled`, `ModuleToggled`, `ModuleThresholdSet`, `RefineryRan`.

**Step 1: Rewrite `useSimStream.ts`**

The key changes:
1. Remove `OreCompositions` type and state
2. `OreMined` event: add the `ore_lot` item to the ship's `inventory` (not cargo); update asteroid remaining mass
3. `OreDeposited` event: extend station `inventory` with `items`; clear ship `inventory`
4. `ModuleInstalled` event: remove `Module` item from station inventory, add `ModuleState` to `station.modules`
5. `ModuleToggled` event: update `module.enabled`
6. `ModuleThresholdSet` event: update `module.kind_state.Processor.threshold_kg`
7. `RefineryRan` event: update station inventory — this is complex because the backend already mutated state. The simplest approach: **re-fetch the full snapshot on `RefineryRan`** OR trust the snapshot and treat `RefineryRan` as a display-only event (show in log but don't mutate FE state). Since the stream sends a full snapshot on connect and then incremental events, the simplest MVP approach is to apply the `RefineryRan` by removing consumed ore and adding material/slag manually.

For MVP, apply `RefineryRan` as follows:
- Consume `ore_consumed_kg` from station's Ore items (FIFO)
- Add a `Material` item with `material_produced_kg` and `material_quality`
- Blend `slag_produced_kg` into existing Slag (or add new)

The existing `applyEvents` pattern should be followed. Here's the updated structure:

```typescript
// Remove OreCompositions entirely from the file.
// The hook return type changes: remove oreCompositions from return value.

// In applyEvents, update the OreMined handler:
case 'OreMined': {
  const { ship_id, asteroid_id, ore_lot, asteroid_remaining_kg } = event
  // Update asteroid
  if (asteroid_remaining_kg <= 0) {
    updatedAsteroids = Object.fromEntries(
      Object.entries(updatedAsteroids).filter(([id]) => id !== asteroid_id)
    )
  } else if (updatedAsteroids[asteroid_id]) {
    updatedAsteroids = {
      ...updatedAsteroids,
      [asteroid_id]: { ...updatedAsteroids[asteroid_id], mass_kg: asteroid_remaining_kg },
    }
  }
  // Add ore lot to ship inventory
  if (updatedShips[ship_id]) {
    updatedShips = {
      ...updatedShips,
      [ship_id]: {
        ...updatedShips[ship_id],
        inventory: [...updatedShips[ship_id].inventory, ore_lot],
      },
    }
  }
  break
}

case 'OreDeposited': {
  const { ship_id, station_id, items } = event
  if (updatedShips[ship_id]) {
    updatedShips = {
      ...updatedShips,
      [ship_id]: { ...updatedShips[ship_id], inventory: [] },
    }
  }
  if (updatedStations[station_id]) {
    updatedStations = {
      ...updatedStations,
      [station_id]: {
        ...updatedStations[station_id],
        inventory: [...updatedStations[station_id].inventory, ...items],
      },
    }
  }
  break
}

case 'ModuleInstalled': {
  const { station_id, module_id, module_item_id, module_def_id } = event
  if (updatedStations[station_id]) {
    const station = updatedStations[station_id]
    updatedStations = {
      ...updatedStations,
      [station_id]: {
        ...station,
        inventory: station.inventory.filter(
          (i) => !(i.kind === 'Module' && i.item_id === module_item_id)
        ),
        modules: [
          ...station.modules,
          {
            id: module_id,
            def_id: module_def_id,
            enabled: false,
            kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0 } },
          },
        ],
      },
    }
  }
  break
}

case 'ModuleToggled': {
  const { station_id, module_id, enabled } = event
  if (updatedStations[station_id]) {
    updatedStations = {
      ...updatedStations,
      [station_id]: {
        ...updatedStations[station_id],
        modules: updatedStations[station_id].modules.map((m) =>
          m.id === module_id ? { ...m, enabled } : m
        ),
      },
    }
  }
  break
}

case 'ModuleThresholdSet': {
  const { station_id, module_id, threshold_kg } = event
  if (updatedStations[station_id]) {
    updatedStations = {
      ...updatedStations,
      [station_id]: {
        ...updatedStations[station_id],
        modules: updatedStations[station_id].modules.map((m) => {
          if (m.id !== module_id) return m
          const ks = m.kind_state
          if (typeof ks === 'object' && 'Processor' in ks) {
            return { ...m, kind_state: { Processor: { ...ks.Processor, threshold_kg } } }
          }
          return m
        }),
      },
    }
  }
  break
}

case 'RefineryRan': {
  const { station_id, ore_consumed_kg, material_produced_kg, material_quality, slag_produced_kg } = event
  // Find the element from the station's modules — for now assume Fe since only one recipe exists
  const REFINERY_ELEMENT = 'Fe'
  if (updatedStations[station_id]) {
    let stationInv = [...updatedStations[station_id].inventory]

    // Consume ore_consumed_kg from Ore items FIFO
    let remaining = ore_consumed_kg
    stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
      if (remaining > 0 && item.kind === 'Ore') {
        const take = Math.min(item.kg, remaining)
        remaining -= take
        if (item.kg - take > 0.001) {
          acc.push({ ...item, kg: item.kg - take })
        }
        return acc
      }
      acc.push(item)
      return acc
    }, [])

    // Add material
    if (material_produced_kg > 0.001) {
      stationInv.push({ kind: 'Material', element: REFINERY_ELEMENT, kg: material_produced_kg, quality: material_quality })
    }

    // Blend slag
    if (slag_produced_kg > 0.001) {
      const existingIdx = stationInv.findIndex((i) => i.kind === 'Slag')
      if (existingIdx >= 0) {
        const existing = stationInv[existingIdx] as SlagItem
        const total = existing.kg + slag_produced_kg
        // Simplified: keep existing composition (full blending needs composition from event — add in future)
        stationInv[existingIdx] = { ...existing, kg: total }
      } else {
        stationInv.push({ kind: 'Slag', kg: slag_produced_kg, composition: {} })
      }
    }

    updatedStations = {
      ...updatedStations,
      [station_id]: { ...updatedStations[station_id], inventory: stationInv },
    }
  }
  break
}
```

Remove `oreCompositions` from the hook's state, initial state, `applyEvents` signature, and return value.

**Step 2: Run type-check**

Run: `cd ui_web && npx tsc --noEmit`
Expected: No errors.

**Step 3: Commit**

```bash
git add ui_web/src/hooks/useSimStream.ts
git commit -m "feat: update useSimStream for inventory model and new events"
```

---

### Task 9: Frontend FleetPanel update

**Files:**
- Modify: `ui_web/src/components/FleetPanel.tsx`
- Modify: `ui_web/src/components/FleetPanel.test.tsx`
- Modify: `ui_web/src/App.tsx` (remove oreCompositions prop)

**Step 1: Rewrite `FleetPanel.tsx`**

Remove the `OreCompositions` import and prop. Render inventory by item kind.

Quality tier helper:
```typescript
function qualityTier(quality: number): string {
  if (quality >= 0.8) return 'excellent'
  if (quality >= 0.5) return 'good'
  return 'poor'
}
```

`InventoryDisplay` component replacing `CargoBreakdown`:
```typescript
function InventoryDisplay({ inventory }: { inventory: InventoryItem[] }) {
  const totalKg = inventory
    .filter((i): i is OreItem | SlagItem | MaterialItem => i.kind !== 'Module' && i.kind !== 'Component' && 'kg' in i)
    .reduce((sum, i) => sum + (i as { kg: number }).kg, 0)

  if (totalKg === 0 && !inventory.some((i) => i.kind === 'Module')) {
    return <div className="text-faint mt-0.5">hold empty</div>
  }

  return (
    <div className="mt-0.5">
      {inventory.map((item, idx) => {
        if (item.kind === 'Ore') {
          return (
            <div key={idx} className="mb-0.5">
              <div className="flex gap-x-2 text-accent">
                <span>ore</span>
                <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
                <span className="text-faint">← {item.asteroid_id}</span>
              </div>
              <div className="flex flex-wrap gap-x-2 text-[10px] text-dim pl-2">
                {Object.entries(item.composition)
                  .sort(([, a], [, b]) => b - a)
                  .filter(([, f]) => f > 0.001)
                  .map(([el, f]) => (
                    <span key={el}>{el} {Math.round(f * 100)}%</span>
                  ))}
              </div>
            </div>
          )
        }
        if (item.kind === 'Material') {
          return (
            <div key={idx} className="flex gap-x-2 text-accent mb-0.5">
              <span>{item.element}</span>
              <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
              <span className="text-faint">{qualityTier(item.quality)}</span>
            </div>
          )
        }
        if (item.kind === 'Slag') {
          return (
            <div key={idx} className="mb-0.5">
              <div className="flex gap-x-2 text-dim">
                <span>slag</span>
                <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
              </div>
              {Object.keys(item.composition).length > 0 && (
                <div className="flex flex-wrap gap-x-2 text-[10px] text-dim pl-2">
                  {Object.entries(item.composition)
                    .sort(([, a], [, b]) => b - a)
                    .filter(([, f]) => f > 0.001)
                    .map(([el, f]) => (
                      <span key={el}>{el} {Math.round(f * 100)}%</span>
                    ))}
                </div>
              )}
            </div>
          )
        }
        if (item.kind === 'Module') {
          return (
            <div key={idx} className="text-faint text-[10px]">
              module: {item.module_def_id}
            </div>
          )
        }
        return null
      })}
    </div>
  )
}
```

For stations, add a modules section:
```typescript
function ModulesDisplay({ modules }: { modules: ModuleState[] }) {
  if (modules.length === 0) return null
  return (
    <div className="mt-1">
      {modules.map((m) => {
        const threshold = typeof m.kind_state === 'object' && 'Processor' in m.kind_state
          ? m.kind_state.Processor.threshold_kg
          : null
        return (
          <div key={m.id} className="text-[10px] text-dim">
            {m.def_id} · {m.enabled ? 'on' : 'off'}
            {threshold !== null && ` · threshold ${threshold} kg`}
          </div>
        )
      })}
    </div>
  )
}
```

Update `FleetPanel` props — remove `oreCompositions`:
```typescript
interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
}
```

Use `InventoryDisplay` and `ModulesDisplay` in the render.

**Step 2: Update `App.tsx`**

Remove `oreCompositions` from the `useSimStream` destructuring and from the `FleetPanel` props.

**Step 3: Update `FleetPanel.test.tsx`**

Remove the `oreCompositions={{}}` prop from all `render(...)` calls. Update test assertions:

```typescript
it('renders ship id', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} />)
  expect(screen.getByText(/ship_0001/)).toBeInTheDocument()
})

it('renders ore item', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} />)
  expect(screen.getByText(/ore/)).toBeInTheDocument()
})

it('renders empty state when no ships', () => {
  render(<FleetPanel ships={{}} stations={{}} />)
  expect(screen.getByText(/no ships/i)).toBeInTheDocument()
})
```

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run`
Expected: All tests pass.

Run: `cd ui_web && npx tsc --noEmit`
Expected: No errors.

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.tsx ui_web/src/components/FleetPanel.test.tsx ui_web/src/App.tsx
git commit -m "feat: update FleetPanel to render inventory items and module status"
```
