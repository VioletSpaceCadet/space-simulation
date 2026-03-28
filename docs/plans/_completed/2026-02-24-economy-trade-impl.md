# Economy & Trade System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add money, import/export trading, ship construction via shipyard, and an economy UI panel.

**Architecture:** Extend sim_core types (balance, commands, events, OutputSpec::Ship, TechEffect), add pricing content file, implement trade execution in engine, ship spawning in station assembler, autopilot thruster import heuristic, and a new Economy UI panel with status bar balance display.

**Tech Stack:** Rust (sim_core, sim_control, sim_world), JSON content files, React 19 + TypeScript + Tailwind v4 (ui_web)

**Design doc:** `docs/plans/2026-02-24-economy-trade-design.md`

---

## Phase 1: Core Types & Content (sim_core + content files)

### Task 1: Add `balance` to GameState and Counters

**Files:**
- Modify: `crates/sim_core/src/types.rs:100-110` (GameState struct)
- Test: existing `crates/sim_world/src/lib.rs` tests + `crates/sim_core/src/engine.rs` tests

**Step 1: Add `balance: f64` field to GameState**

In `crates/sim_core/src/types.rs`, add `balance` to the GameState struct after the `research` field:

```rust
pub struct GameState {
    pub meta: MetaState,
    pub scan_sites: Vec<ScanSite>,
    pub asteroids: HashMap<AsteroidId, AsteroidState>,
    pub ships: HashMap<ShipId, ShipState>,
    pub stations: HashMap<StationId, StationState>,
    pub research: ResearchState,
    pub balance: f64,
    pub counters: Counters,
}
```

**Step 2: Fix all compilation errors**

Every place that constructs a `GameState` needs the new field. Key locations:
- `crates/sim_world/src/lib.rs:309` — `build_initial_state()`: add `balance: 1_000_000_000.0`
- `crates/sim_core/src/engine.rs` — any test fixtures constructing GameState
- All test files constructing GameState directly — add `balance: 1_000_000_000.0`
- Search with: `GameState {` across all `.rs` files

Use `#[serde(default)]` on the `balance` field so existing JSON state files (like `dev_base_state.json`) deserialize without breaking. Default should be 0.0 (loaded states don't auto-get money).

**Step 3: Run tests**

```bash
cargo test
```

All existing tests should pass. The balance field defaults to 0.0 for existing state files and 1B for `build_initial_state()`.

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(core): add balance field to GameState"
```

---

### Task 2: Add pricing types and content loading

**Files:**
- Create: `content/pricing.json`
- Modify: `crates/sim_core/src/types.rs` — add pricing types
- Modify: `crates/sim_world/src/lib.rs:195-241` — load pricing.json into GameContent

**Step 1: Create `content/pricing.json`**

```json
{
  "import_surcharge_per_kg": 100.0,
  "export_surcharge_per_kg": 50.0,
  "items": {
    "ore":                          { "base_price_per_unit": 5.0,        "importable": false, "exportable": false },
    "slag":                         { "base_price_per_unit": 1.0,        "importable": false, "exportable": false },
    "Fe":                           { "base_price_per_unit": 50.0,       "importable": true,  "exportable": true },
    "Si":                           { "base_price_per_unit": 80.0,       "importable": true,  "exportable": true },
    "He":                           { "base_price_per_unit": 200.0,      "importable": true,  "exportable": true },
    "repair_kit":                   { "base_price_per_unit": 8000.0,     "importable": true,  "exportable": true },
    "thruster":                     { "base_price_per_unit": 500000.0,   "importable": true,  "exportable": true },
    "module_basic_iron_refinery":   { "base_price_per_unit": 2000000.0,  "importable": true,  "exportable": true },
    "module_basic_assembler":       { "base_price_per_unit": 3000000.0,  "importable": true,  "exportable": true },
    "module_maintenance_bay":       { "base_price_per_unit": 1500000.0,  "importable": true,  "exportable": true },
    "module_exploration_lab":       { "base_price_per_unit": 2500000.0,  "importable": true,  "exportable": true },
    "module_materials_lab":         { "base_price_per_unit": 2500000.0,  "importable": true,  "exportable": true },
    "module_engineering_lab":       { "base_price_per_unit": 2500000.0,  "importable": true,  "exportable": true },
    "module_shipyard":              { "base_price_per_unit": 10000000.0, "importable": true,  "exportable": true }
  }
}
```

**Step 2: Add pricing types to `crates/sim_core/src/types.rs`**

Add near the other content types (after GameContent or near Constants):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingEntry {
    pub base_price_per_unit: f64,
    pub importable: bool,
    pub exportable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTable {
    pub import_surcharge_per_kg: f64,
    pub export_surcharge_per_kg: f64,
    pub items: HashMap<String, PricingEntry>,
}
```

**Step 3: Add `pricing` to GameContent**

In `crates/sim_core/src/types.rs`, find the GameContent struct and add:

```rust
pub pricing: PricingTable,
```

**Step 4: Load pricing.json in `crates/sim_world/src/lib.rs`**

In `load_content()` (around line 228), add before the `GameContent` construction:

```rust
let pricing: PricingTable = serde_json::from_str(
    &std::fs::read_to_string(dir.join("pricing.json")).context("reading pricing.json")?,
)
.context("parsing pricing.json")?;
```

Add `pricing` to the GameContent struct construction.

**Step 5: Fix compilation errors in test fixtures**

The `base_content()` and `minimal_content()` test fixtures in `crates/sim_core/src/test_fixtures.rs` (or wherever they live) need a `pricing` field. Add a minimal pricing table:

```rust
pricing: PricingTable {
    import_surcharge_per_kg: 100.0,
    export_surcharge_per_kg: 50.0,
    items: HashMap::new(),
},
```

**Step 6: Run tests and commit**

```bash
cargo test
git add -A && git commit -m "feat(core): add pricing types and content loading"
```

---

### Task 3: Add `thruster` component and new modules to content

**Files:**
- Modify: `content/component_defs.json`
- Modify: `content/module_defs.json`
- Modify: `content/techs.json`

**Step 1: Add thruster to `content/component_defs.json`**

Add after the repair_kit entry:

```json
{
  "id": "thruster",
  "name": "Thruster",
  "mass_kg": 200.0,
  "volume_m3": 0.5
}
```

**Step 2: Add engineering lab and shipyard to `content/module_defs.json`**

Add `module_engineering_lab` following the same pattern as existing labs:

```json
{
  "id": "module_engineering_lab",
  "name": "Engineering Lab",
  "mass_kg": 2000.0,
  "volume_m3": 8.0,
  "power_consumption_per_run": 12.0,
  "wear_per_run": 0.005,
  "behavior": {
    "Lab": {
      "domain": "Engineering",
      "data_consumption_per_run": 10.0,
      "research_points_per_run": 5.0,
      "accepted_data": ["EngineeringData", "MiningData"]
    }
  }
}
```

Add `module_shipyard`:

```json
{
  "id": "module_shipyard",
  "name": "Shipyard",
  "mass_kg": 5000.0,
  "volume_m3": 20.0,
  "power_consumption_per_run": 25.0,
  "wear_per_run": 0.02,
  "behavior": {
    "Assembler": {
      "assembly_interval_ticks": 1440,
      "recipes": [
        {
          "id": "recipe_basic_mining_shuttle",
          "inputs": [
            { "filter": { "Element": "Fe" }, "amount": { "Kg": 5000.0 } },
            { "filter": { "Component": "thruster" }, "amount": { "Count": 4 } }
          ],
          "outputs": [
            { "Ship": { "cargo_capacity_m3": 50.0 } }
          ],
          "efficiency": 1.0
        }
      ]
    }
  }
}
```

Note: The `InputFilter::Component` variant does not exist yet. This will be implemented in Task 5. The `OutputSpec::Ship` variant will be implemented in Task 4. The JSON structure above assumes the final serialization format — adjust if Rust serde layout differs.

**Step 3: Add `tech_ship_construction` to `content/techs.json`**

Add to the techs array:

```json
{
  "id": "tech_ship_construction",
  "name": "Ship Construction",
  "prereqs": [],
  "domain_requirements": { "Engineering": 200.0 },
  "accepted_data": ["EngineeringData", "MiningData"],
  "difficulty": 500.0,
  "effects": [{ "EnableShipConstruction": {} }]
}
```

Note: `TechEffect::EnableShipConstruction` variant will be added in Task 4. Don't load content until the Rust types match.

**Step 4: Commit content files (may not parse until Rust types are updated)**

```bash
git add content/ && git commit -m "content: add thruster, engineering lab, shipyard, ship construction tech"
```

---

### Task 4: Add new type variants (OutputSpec::Ship, TechEffect, InputFilter, Command, Event)

**Files:**
- Modify: `crates/sim_core/src/types.rs`

This is the big types task. Add all new enum variants needed.

**Step 1: Add `TechEffect::EnableShipConstruction`**

In `types.rs` around line 552-555, add to the TechEffect enum:

```rust
pub enum TechEffect {
    EnableDeepScan,
    DeepScanCompositionNoise { sigma: f32 },
    EnableShipConstruction,
}
```

**Step 2: Add `InputFilter::Component` variant**

In `types.rs` around line 652-659, add:

```rust
pub enum InputFilter {
    ItemKind(ItemKind),
    Element(ElementId),
    ElementWithMinQuality {
        element: ElementId,
        min_quality: f32,
    },
    Component(ComponentId),
}
```

**Step 3: Add `OutputSpec::Ship` variant**

In `types.rs` around line 676-689, add:

```rust
pub enum OutputSpec {
    Material {
        element: ElementId,
        yield_formula: YieldFormula,
        quality_formula: QualityFormula,
    },
    Slag { yield_formula: YieldFormula },
    Component {
        component_id: ComponentId,
        quality_formula: QualityFormula,
    },
    Ship {
        cargo_capacity_m3: f32,
    },
}
```

**Step 4: Add `TradeItemSpec` enum**

Add a new enum for trade commands:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeItemSpec {
    Material { element: String, kg: f32 },
    Component { component_id: ComponentId, count: u32 },
    Module { module_def_id: String },
}
```

**Step 5: Add `Command::Import` and `Command::Export`**

In the Command enum (around line 309-337), add:

```rust
Import {
    station_id: StationId,
    item_spec: TradeItemSpec,
},
Export {
    station_id: StationId,
    item_spec: TradeItemSpec,
},
```

**Step 6: Add new Event variants**

In the Event enum (around line 351-512), add:

```rust
ItemImported {
    station_id: StationId,
    item_spec: TradeItemSpec,
    cost: f64,
    balance_after: f64,
},
ItemExported {
    station_id: StationId,
    item_spec: TradeItemSpec,
    revenue: f64,
    balance_after: f64,
},
ShipConstructed {
    station_id: StationId,
    ship_id: ShipId,
},
InsufficientFunds {
    station_id: StationId,
    action: String,
    required: f64,
    available: f64,
},
ModuleAwaitingTech {
    station_id: StationId,
    module_id: ModuleInstanceId,
    tech_id: TechId,
},
```

**Step 7: Fix compilation — handle new variants in match statements**

Search for `match` on `Command`, `Event`, `OutputSpec`, `InputFilter`, `TechEffect` across the codebase. Add placeholder arms or proper handling. Key locations:
- `engine.rs` apply_commands — add `Command::Import | Command::Export => {}` stub
- `station.rs` assembler logic — add `OutputSpec::Ship => {}` stub
- `station.rs` input checking — handle `InputFilter::Component`
- Content validation in `sim_world` — handle new variants

**Step 8: Run tests and commit**

```bash
cargo test
git add -A && git commit -m "feat(core): add economy type variants (commands, events, OutputSpec::Ship, trade specs)"
```

---

## Phase 2: Trade Execution (sim_core engine)

### Task 5: Implement InputFilter::Component in assembler

**Files:**
- Modify: `crates/sim_core/src/station.rs` — assembler input checking and consumption
- Create test in: `crates/sim_core/src/station.rs` (or engine test module)

**Step 1: Write a failing test**

Test that an assembler recipe with `InputFilter::Component` input correctly checks for and consumes components from station inventory.

**Step 2: Implement InputFilter::Component handling**

In `station.rs`, the assembler input availability check (around lines 564-599) iterates inputs and checks filters. Add handling for `InputFilter::Component(component_id)`:
- Check station inventory for `InventoryItem::Component` matching the component_id
- Verify count >= required `InputAmount::Count(n)`

In the consumption loop (around lines 733-751), add component consumption:
- Decrement `count` on matching component item
- Remove item if count reaches 0

**Step 3: Run tests and commit**

```bash
cargo test -p sim_core
git add -A && git commit -m "feat(core): implement InputFilter::Component for assembler recipes"
```

---

### Task 6: Implement trade execution (Import/Export commands)

**Files:**
- Modify: `crates/sim_core/src/engine.rs:47-273` — apply_commands function
- Modify: `crates/sim_core/src/types.rs` — may need helper functions for mass lookup
- Test: new tests in engine module

**Step 1: Write failing tests for import**

Test cases:
- Import material (Fe, 100kg) — deducts correct cost, adds to inventory
- Import component (thruster, 2) — deducts cost, adds to inventory
- Import with insufficient funds — emits InsufficientFunds event, no state change
- Import non-importable item (ore) — rejected silently or with event
- Import exceeding cargo capacity — rejected

**Step 2: Write failing tests for export**

Test cases:
- Export material (Fe, 100kg) — removes from inventory, adds revenue
- Export component (repair_kit, 5) — removes, adds revenue
- Export non-exportable item (slag) — rejected
- Export more than available — rejected

**Step 3: Implement pricing helper functions**

Add to engine.rs (or a new `crates/sim_core/src/trade.rs` module):

```rust
fn compute_import_cost(
    item_spec: &TradeItemSpec,
    pricing: &PricingTable,
    content: &GameContent,
) -> Option<f64> {
    // Look up pricing entry, compute base_price * quantity + mass * surcharge
    // Return None if item not found or not importable
}

fn compute_export_revenue(
    item_spec: &TradeItemSpec,
    pricing: &PricingTable,
    content: &GameContent,
) -> Option<f64> {
    // Look up pricing entry, compute base_price * quantity - mass * surcharge, floor at 0
    // Return None if item not found or not exportable
}
```

Mass lookup: materials use 1.0 kg per kg (trivial), components use `component_defs` mass_kg, modules use `module_defs` mass_kg.

**Step 4: Implement Import command handling in apply_commands**

In `engine.rs` apply_commands, handle `Command::Import`:
1. Look up pricing entry — skip if not importable
2. Compute cost via helper
3. Check `state.balance >= cost` — emit InsufficientFunds if not
4. Compute item volume, check station cargo capacity — skip if insufficient
5. Deduct balance
6. Add items to station inventory (Material/Component/Module)
7. Emit ItemImported event

**Step 5: Implement Export command handling**

Handle `Command::Export`:
1. Look up pricing entry — skip if not exportable
2. Check station has the items
3. Compute revenue via helper
4. Remove items from inventory
5. Add revenue to balance
6. Emit ItemExported event

**Step 6: Run tests and commit**

```bash
cargo test -p sim_core
git add -A && git commit -m "feat(core): implement import/export trade execution"
```

---

### Task 7: Implement ship construction (OutputSpec::Ship in assembler)

**Files:**
- Modify: `crates/sim_core/src/station.rs` — assembler output handling
- Modify: `crates/sim_core/src/types.rs` — if ShipState needs changes
- Test: new test in station/engine module

**Step 1: Write a failing test**

Test that when an assembler with a Ship output recipe completes:
- Tech `tech_ship_construction` must be unlocked (stalls with ModuleAwaitingTech if not)
- When unlocked: consumes inputs, spawns new ShipState at station's node
- ShipConstructed event emitted with new ship_id
- New ship is autopilot-owned with correct cargo capacity

**Step 2: Add tech gate check**

In the assembler execution path (station.rs around line 698), before running a recipe that has `OutputSpec::Ship`:
- Check if any output is `OutputSpec::Ship`
- If so, check `state.research.unlocked.contains(&TechId("tech_ship_construction"))`
- If not unlocked, emit `ModuleAwaitingTech` event and return (stall)

**Step 3: Implement OutputSpec::Ship handling**

In the output loop (station.rs around line 759), add a new arm:

```rust
OutputSpec::Ship { cargo_capacity_m3 } => {
    let uuid = sim_core::generate_uuid(rng);
    let ship_id = ShipId(format!("ship_{uuid}"));
    let ship = ShipState {
        id: ship_id.clone(),
        location_node: station.location_node.clone(),
        owner: PrincipalId("principal_autopilot".to_string()),
        inventory: vec![],
        cargo_capacity_m3: *cargo_capacity_m3,
        task: None,
    };
    // Ship must be added to state.ships — need &mut state access.
    // This may require restructuring: collect ships to add, apply after loop.
    new_ships.push((ship_id.clone(), ship));
    events.push(/* ShipConstructed event */);
}
```

Note: The assembler tick function currently only has `&mut station` access, not `&mut state`. Ship spawning needs `state.ships` access. Two approaches:
- **Option A**: Return new ships from the station tick function, apply in engine.rs after tick_stations
- **Option B**: Pass `&mut state.ships` into the assembler tick

Option A is cleaner (station module doesn't reach into global state). The `tick_assembler_modules` return type changes to include spawned ships.

**Step 4: Apply spawned ships in engine.rs**

After `tick_stations()` call in engine.rs, collect any spawned ships and insert them into `state.ships`.

**Step 5: Run tests and commit**

```bash
cargo test -p sim_core
git add -A && git commit -m "feat(core): implement ship construction via OutputSpec::Ship"
```

---

## Phase 3: Content & World Integration

### Task 8: Update dev_base_state.json and build_initial_state()

**Files:**
- Modify: `content/dev_base_state.json`
- Modify: `crates/sim_world/src/lib.rs:243-334`

**Step 1: Add engineering lab and shipyard to dev_base_state.json**

Add to station inventory:

```json
{ "kind": "Module", "item_id": "module_item_0006", "module_def_id": "module_engineering_lab" },
{ "kind": "Module", "item_id": "module_item_0007", "module_def_id": "module_shipyard" }
```

Add balance field to the root:

```json
"balance": 1000000000.0
```

**Step 2: Update build_initial_state() in sim_world**

Add engineering lab and shipyard to the inventory vec. Add `balance: 1_000_000_000.0` to GameState construction.

**Step 3: Run tests and commit**

```bash
cargo test
git add -A && git commit -m "feat(world): add engineering lab, shipyard, and balance to initial state"
```

---

## Phase 4: Autopilot (sim_control)

### Task 9: Autopilot engineering lab assignment and thruster import

**Files:**
- Modify: `crates/sim_control/src/lib.rs`
- Test: existing + new tests

**Step 1: Verify engineering lab auto-assignment works already**

The existing lab assignment logic (lines 211-275) should handle engineering labs automatically — it matches lab domain to tech domain requirements. Write a test confirming `module_engineering_lab` gets assigned to `tech_ship_construction` when eligible.

**Step 2: Write failing test for thruster import heuristic**

Test: when shipyard installed, tech unlocked, station has >= 5000 kg Fe, station has < 4 thrusters, and balance > 2x import cost → autopilot issues Import command for thrusters.

Test: when balance is too low → no import command issued.

**Step 3: Implement thruster import in autopilot**

In `generate_commands()` or `station_module_commands()`, after module install/enable logic, add:

```rust
// Thruster import heuristic
if shipyard_installed && tech_unlocked && fe_kg >= 5000.0 && thruster_count < 4 {
    let import_cost = /* compute from pricing */;
    if state.balance > import_cost * 2.0 {
        commands.push(Command::Import {
            station_id: station.id.clone(),
            item_spec: TradeItemSpec::Component {
                component_id: ComponentId("thruster".to_string()),
                count: 4 - thruster_count,
            },
        });
    }
}
```

**Step 4: Run tests and commit**

```bash
cargo test -p sim_control
git add -A && git commit -m "feat(control): autopilot engineering lab assignment and thruster import"
```

---

## Phase 5: UI

### Task 10: Add balance to frontend types and status bar

**Files:**
- Modify: `ui_web/src/types.ts` — add balance to SimSnapshot
- Modify: `ui_web/src/components/StatusBar.tsx` — display balance
- Modify: `ui_web/src/components/EventsFeed.tsx` — render new event types

**Step 1: Add balance to SimSnapshot type**

In `ui_web/src/types.ts` (around line 157-164), add `balance: number` to SimSnapshot.

**Step 2: Add balance display to StatusBar**

Format as currency with abbreviation (e.g., `$1.00B`, `$523.4M`). Add between tick display and alert badges.

**Step 3: Add new event type rendering**

In EventsFeed, the existing generic renderer (key-value pairs) should handle new events automatically since they serialize as JSON objects. Verify ItemImported, ItemExported, ShipConstructed, InsufficientFunds render correctly.

**Step 4: Run frontend tests and commit**

```bash
cd ui_web && npm test
git add -A && git commit -m "feat(ui): add balance to status bar and new event types"
```

---

### Task 11: Economy panel (import/export UI)

**Files:**
- Create: `ui_web/src/components/EconomyPanel.tsx`
- Modify: `ui_web/src/layout.ts` — add 'economy' panel ID
- Modify: `ui_web/src/App.tsx` — register economy panel

**Step 1: Add 'economy' to panel system**

In `layout.ts`, add `'economy'` to the PanelId union type and PANEL_LABELS map.

In `App.tsx`, add the EconomyPanel to the renderPanel switch.

**Step 2: Create EconomyPanel component**

The panel should display:
- Current balance (large, formatted)
- Import section: dropdown to select item (from pricing table items where importable=true), quantity input, computed cost preview, "Import" button
- Export section: dropdown to select item (from pricing table items where exportable=true), quantity input, computed revenue preview, "Export" button
- Recent transactions: filter events for ItemImported/ItemExported, show last 20

Import/export buttons send POST requests to new API endpoints (or reuse command injection via existing mechanisms). Since the daemon already has a command system, the simplest approach is:
- Add `POST /api/v1/command` endpoint to sim_daemon that accepts a Command JSON body
- Or add `POST /api/v1/import` and `POST /api/v1/export` convenience endpoints

Check what pattern sim_daemon uses for pause/resume (POST endpoints that inject state changes). Follow the same pattern.

**Step 3: Wire up API calls**

The UI needs to send import/export commands to the daemon. Check how pause/resume work in the daemon — they use atomic flags. Trade commands need to go through the command system instead.

Options:
- Add a command queue to the daemon (Arc<Mutex<Vec<CommandEnvelope>>>), POST endpoints push commands, tick loop drains them
- This is the cleanest approach and enables future player commands

**Step 4: Add pricing data to the frontend**

The UI needs pricing data to show cost/revenue previews. Options:
- Add `GET /api/v1/pricing` endpoint that returns the PricingTable
- Or include pricing in the existing `/api/v1/meta` response

**Step 5: Run frontend tests and commit**

```bash
cd ui_web && npm test
git add -A && git commit -m "feat(ui): add Economy panel with import/export controls"
```

---

## Phase 6: Daemon Integration

### Task 12: Add command injection and pricing endpoints to sim_daemon

**Files:**
- Modify: `crates/sim_daemon/src/main.rs` (or routes module)

**Step 1: Add shared command queue**

Add `Arc<Mutex<Vec<CommandEnvelope>>>` to AppState. The tick loop drains this queue each tick and passes commands to `engine::tick()`.

**Step 2: Add POST /api/v1/command endpoint**

Accepts JSON body matching Command enum. Wraps in CommandEnvelope with next command ID. Pushes to queue.

**Step 3: Add GET /api/v1/pricing endpoint**

Returns the PricingTable from GameContent as JSON.

**Step 4: Include balance in snapshot response**

Verify `/api/v1/snapshot` already includes balance (it should, since it serializes GameState which now has the field).

**Step 5: Run daemon tests and commit**

```bash
cargo test -p sim_daemon
git add -A && git commit -m "feat(daemon): add command injection queue and pricing endpoint"
```

---

## Phase 7: Integration Testing & Docs

### Task 13: End-to-end integration test

**Files:**
- Add test in `crates/sim_core/src/engine.rs` or new integration test file

**Step 1: Write full-loop integration test**

Test the complete economy flow:
1. Start with initial state (balance = 1B, no thrusters)
2. Run enough ticks for engineering lab to generate research points
3. Manually unlock tech_ship_construction (or run enough ticks)
4. Issue Import command for thrusters
5. Verify balance deducted, thrusters in inventory
6. Run shipyard until completion
7. Verify new ship spawned, ShipConstructed event emitted
8. Issue Export command for Fe
9. Verify balance increased, Fe removed

**Step 2: Run all tests**

```bash
cargo test
cd ui_web && npm test
```

**Step 3: Commit**

```bash
git add -A && git commit -m "test: end-to-end economy integration test"
```

---

### Task 14: Update documentation

**Files:**
- Modify: `CLAUDE.md` — update architecture section
- Modify: `docs/reference.md` — add economy types

**Step 1: Update CLAUDE.md**

- Add economy commands and events to sim_core description
- Add pricing.json to content files list
- Add economy panel to ui_web description
- Update tick order if needed
- Add new constants/content files

**Step 2: Update docs/reference.md**

Add economy types, pricing table format, trade restrictions.

**Step 3: Commit**

```bash
git add -A && git commit -m "docs: update CLAUDE.md and reference.md for economy system"
```

---

## Task Dependency Graph

```
Task 1 (balance field)
  └→ Task 2 (pricing types)
       └→ Task 3 (content files) ─────────────────┐
       └→ Task 4 (type variants) ─────────────────┤
            └→ Task 5 (InputFilter::Component)     │
            └→ Task 6 (trade execution)            │
            └→ Task 7 (ship construction)          │
                 └→ Task 8 (initial state) ←───────┘
                      └→ Task 9 (autopilot)
                           └→ Task 10 (UI types + status bar)
                                └→ Task 11 (economy panel)
                                └→ Task 12 (daemon endpoints)
                                     └→ Task 13 (integration test)
                                          └→ Task 14 (docs)
```

Tasks 5, 6, 7 can be parallelized after Task 4.
Tasks 10, 11, 12 can be partially parallelized after Task 9.
