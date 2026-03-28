# Economy & Trade System Design

## Overview

Add money, import/export trading, ship construction, and a shipyard module to create an economy loop. Players earn money by exporting refined materials and components, spend money importing supplies (notably thrusters), and invest in ship construction to scale operations.

## Data Model

### GameState additions

- `balance: f64` on `GameState` — global money balance, starts at 1,000,000,000.0

### New content file: `content/pricing.json`

Flat lookup table mapping item identifiers to pricing metadata:

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

### Pricing formulas

- **Import cost** = `base_price * quantity + total_mass_kg * import_surcharge_per_kg`
- **Export revenue** = `base_price * quantity - total_mass_kg * export_surcharge_per_kg` (floored at 0)

Mass surcharges create friction: lightweight high-value items (thrusters, components) are profitable to trade, while bulk materials have thin margins.

### Trade restrictions

| Item | Importable | Exportable | Rationale |
|------|-----------|-----------|-----------|
| Ore | No | No | Must be mined |
| Slag | No | No | Waste; creates inventory pressure |
| Fe, Si, He | Yes | Yes | Core tradeable materials |
| repair_kit | Yes | Yes | Manufactured component |
| thruster | Yes | Yes | Currently import-only sourcing (no recipe), but can be sold back |
| All modules | Yes | Yes | Station equipment |

## Commands

New `Command` variants:

```rust
Command::Import { station_id, item_spec, quantity }
Command::Export { station_id, item_spec, quantity }
```

`item_spec` is a new enum:

```rust
enum TradeItemSpec {
    Material { element: String, kg: f32 },
    Component { component_id: ComponentId, count: u32 },
    Module { module_def_id: String },
}
```

### Command execution (tick step 1: Apply commands)

**Import:**
1. Look up pricing entry; reject if `importable: false`
2. Compute cost = base_price * quantity + mass * import_surcharge
3. Check `balance >= cost`; emit `InsufficientFunds` if not
4. Check station cargo capacity; reject if insufficient
5. Deduct balance, add items to station inventory
6. Emit `ItemImported` event

**Export:**
1. Look up pricing entry; reject if `exportable: false`
2. Check station has the items in sufficient quantity
3. Compute revenue = base_price * quantity - mass * export_surcharge (floor 0)
4. Remove items from station inventory, add revenue to balance
5. Emit `ItemExported` event

## Events

New `Event` variants:

```rust
Event::ItemImported { station_id, item_spec, quantity, cost, balance_after }
Event::ItemExported { station_id, item_spec, quantity, revenue, balance_after }
Event::ShipConstructed { station_id, ship_id }
Event::InsufficientFunds { station_id, action, required, available }
Event::ModuleAwaitingTech { station_id, module_id, tech_id }
```

## Ship Construction

### New tech: `tech_ship_construction`

- Domain requirements: `{ Engineering: 200.0 }`
- Accepted data: `[EngineeringData, MiningData]`
- Difficulty: 500.0
- Prereqs: none (parallel research path)
- Effects: `[EnableShipConstruction]`

### New TechEffect variant

```rust
TechEffect::EnableShipConstruction
```

### New module: `module_engineering_lab`

- Behavior: `Lab { domain: Engineering }`
- Same pattern as exploration/materials labs
- `data_consumption_per_run`, `research_points_per_run` matching other labs

### New module: `module_shipyard`

- Behavior: `Assembler`
- `assembly_interval_ticks`: 1440 (1 day)
- `power_consumption_per_run`: 25.0
- `wear_per_run`: 0.02
- Recipe `recipe_basic_mining_shuttle`:
  - Inputs: 5,000 kg Fe + 4 thrusters
  - Output: `OutputSpec::Ship`

### New OutputSpec variant

```rust
OutputSpec::Ship { ship_def: ShipBlueprint }
```

Where `ShipBlueprint` defines the spawned ship's stats (cargo capacity, etc.). For v1 this is hardcoded to match the existing starter ship.

### Assembler gating

Before running a shipyard recipe with `OutputSpec::Ship`:
1. Check `tech_ship_construction` in `research.unlocked`
2. If not unlocked, emit `ModuleAwaitingTech` and stall (reuse existing stall machinery)

### Ship spawn on completion

When assembler detects `OutputSpec::Ship`:
1. Generate new `ShipId` using existing deterministic UUID generation
2. Create `ShipState` at station's node, owned by `principal_autopilot`
3. Emit `ShipConstructed` event
4. Autopilot picks it up next tick (existing survey/mine/deposit logic)

## New component: `thruster`

Added to `component_defs.json`:

```json
{ "id": "thruster", "name": "Thruster", "mass_kg": 200.0, "volume_m3": 0.5 }
```

No crafting recipe — sourced via import only (for now).

## Autopilot Changes

### Engineering lab

Auto-install and auto-assign to `tech_ship_construction` — same pattern as existing lab assignment logic.

### Shipyard

Auto-install module — same as existing auto-install logic.

### Thruster import heuristic (simple v1)

Only import thrusters when ALL conditions met:
- Shipyard installed and tech unlocked
- Station has >= 5,000 kg Fe (recipe ready)
- Station does not already have >= 4 thrusters
- Balance > 2x total import cost of 4 thrusters

No prioritization against other actions. Simple "can we and should we?" check.

## UI Changes

### Status bar

- Display formatted balance (e.g. `$1.00B`) next to tick display

### New Economy Panel

Draggable panel alongside existing panels:
- Current balance (prominent)
- Recent transactions (last ~20 import/export events)
- Import/export controls: item selector, quantity input, cost/revenue preview, confirm button

### Existing panels

- **Fleet**: new ships appear automatically via snapshot
- **Research**: `tech_ship_construction` + Engineering domain points appear automatically
- **Events**: new event types render with appropriate formatting

## Content File Changes

| File | Change |
|------|--------|
| `content/pricing.json` | New file — price table + surcharges |
| `content/component_defs.json` | Add `thruster` |
| `content/module_defs.json` | Add `module_shipyard` + `module_engineering_lab` |
| `content/techs.json` | Add `tech_ship_construction` |
| `content/dev_base_state.json` | Add engineering lab + shipyard to starting inventory |

## Crate Changes

| Crate | Change |
|-------|--------|
| **sim_core** | `balance` on GameState, `Command::Import/Export`, new events, `OutputSpec::Ship`, `TechEffect::EnableShipConstruction`, pricing types, trade execution in engine, ship spawn in station assembler |
| **sim_control** | Autopilot: engineering lab assignment, shipyard install, thruster import heuristic |
| **sim_world** | Load `pricing.json` into GameContent, update `build_initial_state()` with new modules + starting balance |
| **sim_daemon** | No changes needed (SSE serialization automatic) |
| **ui_web** | Economy panel, status bar balance, new event rendering |

## Tick Order

Unchanged. Import/export commands execute in step 1 (Apply commands). Shipyard runs in step 3b (Tick assemblers). No new tick phases.

## What This Does NOT Include

- Dynamic pricing / supply-demand curves
- Multi-station economies or inter-station transfers
- Player-owned vs NPC-owned ships
- Ship types beyond the starter mining shuttle
- MCTS or advanced autopilot decision-making
- Ore/slag trading (intentionally restricted)
- Credit/debt system (balance floors at 0)
