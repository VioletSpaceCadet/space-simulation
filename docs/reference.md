# Reference — Types, Content, Inventory

Detailed reference for sim_core types, content files, and inventory/refinery mechanics. See CLAUDE.md for architecture overview and commands.

## Key Types (sim_core)

| Type | Purpose |
|---|---|
| `GameState` | Full mutable simulation state (meta, scan_sites, asteroids, ships, stations, research, counters) |
| `ScanSite` | Unscanned potential asteroid location (consumed on survey) |
| `AsteroidState` | Created on discovery; holds `true_composition` (hidden), `knowledge`, `mass_kg`, `anomaly_tags` |
| `ResearchState` | `unlocked`, `data_pool`, `evidence` — no active allocations |
| `ShipState` | `id`, `location_node`, `owner`, `inventory: Vec<InventoryItem>`, `cargo_capacity_m3`, `task` |
| `StationState` | `id`, `location_node`, `inventory`, `cargo_capacity_m3`, `power_available_per_tick`, `facilities`, `modules: Vec<ModuleState>` |
| `InventoryItem` | Enum: `Ore { lot_id, asteroid_id, kg, composition }`, `Material { element, kg, quality }`, `Slag { kg, composition }`, `Component { component_id, count, quality }`, `Module { item_id, module_def_id }` |
| `ModuleState` | Installed module: `id`, `def_id`, `enabled`, `kind_state` (Processor or Storage). Processor has `stalled: bool` |
| `TaskKind` | `Idle`, `Survey`, `DeepScan`, `Mine { asteroid, duration_ticks }`, `Deposit { station, blocked }`, `Transit { destination, total_ticks, then }` |
| `Command` | `AssignShipTask`, `InstallModule`, `UninstallModule`, `SetModuleEnabled`, `SetModuleThreshold` |
| `GameContent` | Static config: techs, solar system, asteroid templates, elements, module_defs, constants |
| `ModuleDef` | Module definition with `ModuleBehaviorDef` (Processor with recipes, or Storage) |
| `TechEffect` | `EnableDeepScan` or `DeepScanCompositionNoise { sigma }` |

## Content Files

All in `content/`. Loaded at runtime; never compiled in.

| File | Key fields |
|---|---|
| `constants.json` | Scan durations, travel ticks, mining rate, cargo capacities, deposit ticks, research compute |
| `techs.json` | Tech tree (`tech_deep_scan_v1` is the only current tech) |
| `solar_system.json` | 4 nodes (Earth Orbit → Inner Belt → Mid Belt → Outer Belt), linear chain |
| `asteroid_templates.json` | 2 templates: `tmpl_iron_rich` (IronRich, Fe-heavy) and `tmpl_silicate` (Si-heavy) |
| `elements.json` | 5 elements: `ore` (3000), `slag` (2500), `Fe` (7874), `Si` (2329), `He` (125) kg/m³ |
| `module_defs.json` | 1 module: `module_basic_iron_refinery` — Processor, 60-tick interval, consumes 1000kg ore, outputs Fe material + slag |
| `dev_base_state.json` | Pre-baked dev state: tick 0, 1 ship, 1 station with refinery module in inventory |

## Inventory & Refinery Design

**Inventory model:** Ships and stations carry `Vec<InventoryItem>` (not HashMap). Volume constraint: `inventory_volume_m3(items, content) ≤ capacity_m3`. Each item type computes volume differently (ore/slag/material by density, components by count, modules by def).

**Ore:** Mining produces `InventoryItem::Ore` with a `lot_id`, `asteroid_id`, `kg`, and snapshot of the asteroid's composition (deep-scanned if available, else true composition). Each asteroid produces distinct ore lots.

**Refinery:** Station modules with `ModuleBehaviorDef::Processor` tick at their defined interval. A processor: checks enabled + power + ore threshold → FIFO-consumes ore up to rate_kg → produces `Material` (element fraction × kg, quality from formula) + `Slag` (remainder). Materials of same element+quality merge. Slag merges into a single accumulating lot.

**Storage enforcement:** Modules and ships respect station cargo capacity.
- **Processor stall:** Before running, a processor estimates its output volume. If the output would exceed the station's remaining capacity, the processor sets `stalled = true` and emits `ModuleStalled { station_id, module_id, shortfall_m3 }`. On the next tick where space is available, it clears the stall and emits `ModuleResumed { station_id, module_id }`. Stall events are emitted only on transition (not every tick).
- **Deposit blocking:** When a ship with a `Deposit` task arrives and there is not enough station capacity for its cargo, the task sets `blocked = true` and emits `DepositBlocked { ship_id, station_id, shortfall_m3 }`. If partial space is available, a partial deposit occurs (FIFO by inventory order). When full space opens, the remaining cargo is deposited and `DepositUnblocked { ship_id, station_id }` is emitted.
- **Metric:** `refinery_stalled_count` — number of processor modules currently in `stalled = true` state.

**Future direction (not yet built):**
- Ore keyed by composition hash instead of asteroid ID — compatible ores blend naturally.
- Blending tolerance as a tech unlock: ±2% basic, ±10% advanced.
- Volatiles flag ore as unblendable.
- Component output from processors (type defined but no-op).
- Storage modules (type defined but tick loop skips them).

## MVP Scope

- **MVP-0 (done):** sim_core tick + tests, sim_control autopilot, sim_cli run loop.
- **MVP-1 (done):** sim_daemon HTTP server, SSE event stream, React mission control UI.
- **MVP-2 (done):** Mining loop — cargo holds, Mine/Deposit tasks, ore extraction.
- **MVP-3 (done):** Refinery system — inventory model, station modules, processor tick logic.
- **FE Foundations (done):** Nav sidebar, sortable tables, color refresh.
- **Solar System Map (done):** SVG orbital map, d3-zoom, entity markers, tooltips, detail cards.
- **Smooth Streaming (done):** useAnimatedTick, 200ms heartbeat, measured tick rate.
- **Procedural Sites (done):** Scan site replenishment with deterministic UUIDs.
- **Storage Enforcement (done):** Processors stall when output exceeds capacity; ships wait when deposit blocked; partial deposits.
