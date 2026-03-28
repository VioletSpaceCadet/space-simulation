# Refinery System Design

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:writing-plans to create the implementation plan from this design.

**Goal:** Add a refinery system — rich inventory, station modules, and a basic iron refinery that processes raw ore into Fe material (with quality) and blended slag.

**Architecture:** Stations and ships replace the flat `HashMap<ElementId, f32>` cargo with a `Vec<InventoryItem>` enum that carries per-item metadata (composition, quality). Station modules are physical inventory items that the autopilot auto-installs; the refinery runs on a tick interval when enabled and above a threshold. A `dev_base_state.json` provides a hand-crafted starting world, establishing the save/load foundation.

**Tech Stack:** Rust (sim_core, sim_control, sim_cli), TypeScript/React (ui_web), serde_json for save/load.

---

## 1. Rich Inventory System

Replace `cargo: HashMap<ElementId, f32>` on both `ShipState` and `StationState` with `inventory: Vec<InventoryItem>`.

```rust
#[serde(tag = "kind")]
pub enum InventoryItem {
    Ore {
        lot_id: LotId,
        asteroid_id: AsteroidId,
        kg: f32,
        composition: CompositionVec,   // snapshotted at mine-time
    },
    Slag {
        kg: f32,
        composition: CompositionVec,   // weighted-average blended across all deposits
    },
    Material {
        element: ElementId,
        kg: f32,
        quality: f32,                  // 0.0–1.0; display tier (poor/good/excellent) is UI only
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
```

- `LotId`, `ModuleItemId`, `ComponentId` are new `string_id!` newtypes; `next_lot_id` added to `Counters`.
- Volume: each item computes volume via its element's density in content. Ore uses `"ore"` density (3000 kg/m³), Slag uses `"slag"` element def, Material uses its element's density, Module uses `ModuleDef.volume_m3`.
- `cargo_volume_used` → `inventory_volume_m3(items: &[InventoryItem], content: &GameContent) -> f32`.
- The `ore:{asteroid_id}` string-key hack is removed. `OreCompositions` on the FE is removed — composition is embedded in each `Ore` item.
- Ore composition is snapshotted from `asteroid.knowledge.composition` at mine-time (falls back to `true_composition` if no deep scan has been done).

**New element defs** (in `content/elements.json`):
- `slag`: density 2500 kg/m³, display_name "Slag"

**New field on `ElementDef`:**
```rust
pub refined_name: String,  // e.g. "Iron Ingot", "Liquid Helium", "Water"
```

---

## 2. Module System

### Content (`content/module_defs.json`)

```rust
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
    pub power_consumption_per_run: f32,
    pub behavior: ModuleBehaviorDef,
}

pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
    // future: PowerGenerator, Shipyard, ResearchLab
}

pub struct ProcessorDef {
    pub processing_interval_ticks: u64,   // 60 = once per in-game hour
    pub recipes: Vec<RecipeDef>,
}

pub struct RecipeDef {
    pub id: String,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<OutputSpec>,
    pub efficiency: f32,
}

pub struct RecipeInput {
    pub filter: InputFilter,
    pub amount: InputAmount,
}

pub enum InputFilter {
    ItemKind(ItemKind),
    Element(ElementId),
    ElementWithMinQuality { element: ElementId, min_quality: f32 },
    // future: Component(ComponentId), And(Vec<InputFilter>)
}

pub enum ItemKind { Ore, Slag, Material, Component }

pub enum InputAmount {
    Kg(f32),
    Count(u32),
}

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

pub enum YieldFormula {
    ElementFraction { element: ElementId },  // kg_out = kg_in × element_fraction
    FixedFraction(f32),                       // kg_out = kg_in × fraction
}

pub enum QualityFormula {
    ElementFractionTimesMultiplier { element: ElementId, multiplier: f32 },
    Fixed(f32),
}
```

**Basic iron refinery content** (`module_defs.json`):
```json
[{
  "id": "module_basic_iron_refinery",
  "name": "Basic Iron Refinery",
  "mass_kg": 5000.0,
  "volume_m3": 10.0,
  "power_consumption_per_run": 10.0,
  "behavior": {
    "Processor": {
      "processing_interval_ticks": 60,
      "recipes": [{
        "id": "recipe_basic_iron",
        "inputs": [{ "filter": { "ItemKind": "Ore" }, "amount": { "Kg": 1000.0 } }],
        "outputs": [
          {
            "Material": {
              "element": "Fe",
              "yield_formula": { "ElementFraction": { "element": "Fe" } },
              "quality_formula": { "ElementFractionTimesMultiplier": { "element": "Fe", "multiplier": 1.0 } }
            }
          },
          { "Slag": { "yield_formula": { "FixedFraction": 1.0 } } }
        ],
        "efficiency": 1.0
      }]
    }
  }
}]
```

### State (`types.rs`)

```rust
pub struct ModuleState {
    pub id: ModuleInstanceId,
    pub def_id: String,
    pub enabled: bool,
    pub kind_state: ModuleKindState,
}

pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
}

pub struct ProcessorState {
    pub threshold_kg: f32,
    pub ticks_since_last_run: u64,
}
```

`StationState` gains `modules: Vec<ModuleState>`.

---

## 3. Refinery Logic & Tick Integration

Per-tick station processing runs after ship tasks resolve, in a new `tick_stations` function in `sim_core`:

```
for each station:
  for each module (enabled):
    increment ticks_since_last_run
    if ticks_since_last_run < processing_interval_ticks → skip
    if station.power_available_per_tick < def.power_consumption_per_run → skip
    match behavior:
      Processor → resolve_processor_run(station, module_state, def, events)
      Storage   → (no per-tick action)
```

**`resolve_processor_run` for the basic iron refinery:**

1. Collect all `InventoryItem::Ore` in station inventory matching input filter; total their kg.
2. If total ore kg < `threshold_kg` → skip (do not reset timer).
3. Consume up to `processing_rate_kg_per_run` (1000 kg) from ore lots, FIFO (oldest `lot_id` first). Partially consume lots — reduce lot kg, remove if depleted.
4. Compute weighted-average `ore_fe_fraction` across consumed lots (weighted by kg consumed from each).
5. Outputs:
   - **Material**: `consumed_kg × ore_fe_fraction` kg, quality = `ore_fe_fraction × multiplier` (clamped 0–1).
   - **Slag**: `consumed_kg × (1 - ore_fe_fraction)` kg, composition = non-Fe fractions normalized to 1.0.
6. Blend new slag into existing `InventoryItem::Slag` by weighted average:
   ```
   blended[el] = (existing_comp[el] × existing_kg + new_comp[el] × new_kg) / (existing_kg + new_kg)
   ```
   If no slag exists yet, insert a new `Slag` item.
7. Append `InventoryItem::Material { element: "Fe", kg, quality }` to station inventory.
8. Reset `ticks_since_last_run = 0`.
9. Emit `RefineryRan`.

---

## 4. Commands, Events & Auto-Install

### New commands

```rust
InstallModule      { station_id: StationId, module_item_id: ModuleItemId },
UninstallModule    { station_id: StationId, module_id: ModuleInstanceId },  // returns item to inventory
SetModuleEnabled   { station_id: StationId, module_id: ModuleInstanceId, enabled: bool },
SetModuleThreshold { station_id: StationId, module_id: ModuleInstanceId, threshold_kg: f32 },
```

### New events

```rust
ModuleInstalled    { station_id: StationId, module_id: ModuleInstanceId, module_item_id: ModuleItemId, module_def_id: String },
ModuleUninstalled  { station_id: StationId, module_id: ModuleInstanceId, module_item_id: ModuleItemId },
ModuleToggled      { station_id: StationId, module_id: ModuleInstanceId, enabled: bool },
ModuleThresholdSet { station_id: StationId, module_id: ModuleInstanceId, threshold_kg: f32 },
RefineryRan {
    station_id: StationId,
    module_id: ModuleInstanceId,
    ore_consumed_kg: f32,
    material_produced_kg: f32,
    material_quality: f32,
    slag_produced_kg: f32,
},
```

### Auto-install rule (autopilot in `sim_control`)

```
for each station:
  for each InventoryItem::Module in station.inventory:
    issue InstallModule { station_id, module_item_id }

for each newly installed module (from ModuleInstalled events this tick):
  issue SetModuleEnabled { ..., enabled: true }
  issue SetModuleThreshold { ..., threshold_kg: 500.0 }
```

---

## 5. Save/Load Foundation (`dev_base_state.json`)

`sim_cli` accepts mutually exclusive startup modes:

```
sim run                        # procedural world gen, random seed (existing)
sim run --seed <n>             # procedural world gen, fixed seed (existing)
sim run --state <path>         # load GameState from JSON file
```

`GameState` is already `Serialize + Deserialize`, so loading is `serde_json::from_reader`. Saving is a future follow-up.

`content/dev_base_state.json` is a hand-crafted tick-0 `GameState` with:
- One ship at `node_earth_orbit`, empty inventory
- One station at `node_earth_orbit`, inventory containing one `Module { module_basic_iron_refinery }`
- Scan sites spread across the solar system (same as procedural gen would produce)
- `schema_version: 1`, `content_version` matching current content

The module starts in station inventory (not in `modules`). Autopilot installs it on tick 1 via the auto-install rule.

---

## 6. Frontend Changes

- `ShipState.cargo` → `inventory: InventoryItem[]` (tagged union matching Rust enum)
- `StationState.cargo` → `inventory: InventoryItem[]`
- `OreCompositions` hook state and prop removed — composition embedded in `Ore` items
- `applyEvents` updated: `OreMined` produces `Ore` items; `OreDeposited` moves items; `RefineryRan` updates station inventory (consume ore lots, add `Material`, blend `Slag`)
- `FleetPanel` updated to render inventory by item kind: ore lots with composition %, materials with quality tier label, slag with composition %, modules listed separately
- New module status section per station: shows installed modules, enabled state, threshold
- `types.ts` updated with full `InventoryItem` discriminated union and `ModuleState` types
