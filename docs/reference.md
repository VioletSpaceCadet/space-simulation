# Reference — Types, Content, Inventory

Detailed reference for sim_core types, content files, and inventory/refinery mechanics. See CLAUDE.md for architecture overview and commands.

## Key Types (sim_core)

| Type | Purpose |
|---|---|
| `GameState` | Full mutable simulation state (meta, scan_sites, asteroids, ships, stations, research, counters) |
| `ScanSite` | Unscanned potential asteroid location (consumed on survey) |
| `AsteroidState` | Created on discovery; holds `true_composition` (hidden), `knowledge`, `mass_kg`, `anomaly_tags` |
| `ResearchState` | `unlocked`, `data_pool`, `evidence` — no active allocations |
| `ShipState` | `id`, `position`, `owner`, `inventory: Vec<InventoryItem>`, `cargo_capacity_m3`, `task`, `speed_ticks_per_au: Option<u64>`, `modifiers` |
| `StationState` | `id`, `location_node`, `inventory`, `cargo_capacity_m3`, `power_available_per_tick`, `facilities`, `modules: Vec<ModuleState>` |
| `InventoryItem` | Enum: `Ore { lot_id, asteroid_id, kg, composition }`, `Material { element, kg, quality }`, `Slag { kg, composition }`, `Component { component_id, count, quality }`, `Module { item_id, module_def_id }` |
| `ModuleState` | Installed module: `id`, `def_id`, `enabled`, `kind_state` (Processor, Storage, Maintenance, Assembler, Lab, SensorArray, SolarArray, Battery, Radiator), `wear: WearState`, optional `thermal: ThermalState`. Processor/Assembler have `stalled: bool`. Assembler also has `capped: bool`, `cap_override: HashMap<ComponentId, u32>` |
| `WearState` | `wear: f32` (0.0–1.0). Embedded on any wearable entity. |
| `TaskKind` | `Idle`, `Survey`, `DeepScan`, `Mine { asteroid, duration_ticks }`, `Deposit { station, blocked }`, `Transit { destination, total_ticks, then }` |
| `Command` | `AssignShipTask`, `InstallModule`, `UninstallModule`, `SetModuleEnabled`, `SetModuleThreshold`, `AssignLabTech`, `SetAssemblerCap`, `Import`, `Export`, `JettisonSlag` |
| `GameContent` | Static config: techs, solar system, asteroid templates, elements, module_defs, component_defs, recipes (`BTreeMap<RecipeId, RecipeDef>`), constants |
| `ModuleDef` | Module definition with `ModuleBehaviorDef` (Processor, Storage, Maintenance, Assembler, Lab, SensorArray, SolarArray, Battery, Radiator), `wear_per_run`, optional `thermal: ThermalDef` |
| `ComponentDef` | Component definition: `id`, `name`, `mass_kg`, `volume_m3` |
| `MaintenanceDef` | Maintenance module behavior: `repair_interval_ticks`, `wear_reduction_per_run`, `repair_kit_cost` |
| `AssemblerDef` | Assembler module behavior: `assembly_interval_ticks`, `recipes: Vec<RecipeId>` (references into `GameContent.recipes`), `max_stock: HashMap<ComponentId, u32>` (optional stock cap per output component) |
| `PricingTable` | `import_surcharge_per_kg`, `export_surcharge_per_kg`, `items: HashMap<String, PricingEntry>` |
| `PricingEntry` | `base_price_per_unit`, `importable`, `exportable` |
| `TradeItemSpec` | Enum: `Material { element, kg }`, `Component { component_id, count }`, `Module { module_def_id }` |
| `OutputSpec` | Enum: `Material { ... }`, `Slag { ... }`, `Component { ... }`, `Ship { cargo_capacity_m3 }` |
| `TechEffect` | `EnableDeepScan`, `DeepScanCompositionNoise { sigma }`, `EnableShipConstruction`, or `StatModifier { stat, op, value }` — numeric bonuses from research |
| `ResearchDomain` | Enum: `Survey`, `Materials`, `Manufacturing`, `Propulsion` — categorises techs and lab output |
| `DomainProgress` | Per-tech domain point tracking: `points: HashMap<ResearchDomain, f64>` |
| `DataKind` | Enum: `SurveyData`, `AssayData`, `ManufacturingData`, `TransitData` — type of raw data a lab consumes |
| `LabDef` | Lab module behavior definition: `data_kind`, `domain`, `throughput_per_tick` |
| `LabState` | Lab runtime state (embedded in `ModuleState::kind_state`): `assigned_tech: Option<TechId>` |
| `ThermalDef` | Module thermal properties: `heat_capacity_j_per_k`, `passive_cooling_coefficient`, `max_temp_mk`, `operating_min_mk`, `operating_max_mk`, `thermal_group`, `idle_heat_generation_w` |
| `ThermalState` | Per-module thermal runtime: `temp_mk: u32`, `thermal_group`, `overheat_zone: OverheatZone`, `overheat_disabled: bool` |
| `OverheatZone` | Enum: `Nominal`, `Warning`, `Critical` — drives wear multiplier and auto-disable |
| `RecipeThermalReq` | Recipe thermal requirements: `min_temp_mk`, `optimal_min_mk`, `optimal_max_mk`, `max_temp_mk`, `heat_per_run_j` |
| `RadiatorDef` | Radiator behavior: `cooling_capacity_w` — shared across thermal group |
| `MaterialThermalProps` | Material thermal tracking: `temp_mk`, `phase: Phase`, `latent_heat_buffer_j` |
| `Phase` | Enum: `Solid`, `Liquid` |

Note: `FacilitiesState` has been removed. Research state is fully contained in `ResearchState`.

## Research System

**Lab-based domain model:** Labs are station modules (`ModuleBehaviorDef::Lab`) that consume raw data from the sim-wide `ResearchState.data_pool` each tick and produce domain-specific research points toward an assigned tech.

**Research Domains (4):**

| Domain | Fantasy | Gameplay Loop |
|--------|---------|---------------|
| Survey | Finding and characterizing the environment | Scanning, sensor arrays |
| Materials | Physical/chemical science | Processing ore, refining |
| Manufacturing | Building systems at scale | Assembly, module operations |
| Propulsion | Moving mass through space | Ship transits (late-game) |

**Data Kinds (4):**

| DataKind | Source Activities |
|----------|------------------|
| SurveyData | Survey tasks, deep scans, sensor arrays |
| AssayData | Mining task completion |
| ManufacturingData | Assembler runs |
| TransitData | Ship transit completion |

**`ResearchState` fields:** `unlocked: HashSet<TechId>`, `data_pool: HashMap<DataKind, f64>`, `evidence: HashMap<TechId, DomainProgress>`, `action_counts: HashMap<String, u32>`.

**Raw data generation:** Tasks generate raw data via `generate_data(research, kind, action_key, constants)` with diminishing returns (yield = `base_yield × (1 / (1 + 0.1 × count))`). Data is sim-wide — not stored in ship or station inventory.

**Research unlock:** Checked every tick. For each eligible tech (prereqs met, not yet unlocked), if all `domain_requirements` are met (`evidence[tech].points[domain] >= requirement` for every domain), the tech unlocks immediately. Techs with no domain requirements unlock as soon as prereqs are met. Processing order is sorted by tech ID for determinism.

**Constants (in `constants.json`):**

| Constant | Purpose |
|---|---|
| `data_generation_peak` | Peak raw data yield per ship task |
| `data_generation_floor` | Minimum raw data yield per ship task |
| `data_generation_decay_rate` | Exponential decay rate for data yield over repeated tasks |

## Content Files

All in `content/`. Loaded at runtime; never compiled in.

| File | Key fields |
|---|---|
| `constants.json` | Scan durations, travel ticks, mining rate, cargo capacities, deposit ticks, research compute |
| `techs.json` | Tech tree (`tech_deep_scan_v1` is the only current tech) |
| `solar_system.json` | 4 nodes (Earth Orbit → Inner Belt → Mid Belt → Outer Belt), linear chain |
| `asteroid_templates.json` | 2 templates: `tmpl_iron_rich` (IronRich, Fe-heavy) and `tmpl_silicate` (Si-heavy) |
| `elements.json` | 5 elements: `ore` (3000), `slag` (2500), `Fe` (7874), `Si` (2329), `He` (125) kg/m³ |
| `module_defs.json` | Modules include: `module_basic_iron_refinery` (Processor, 60-tick interval, wear_per_run=0.01), `module_maintenance_bay` (Maintenance, 30-tick interval, reduces 0.2 wear, costs 1 RepairKit), `module_basic_assembler` (Assembler, 360-tick interval, wear_per_run=0.008, 200kg Fe → 1 RepairKit, max_stock: repair_kit=50), `module_basic_smelter` (Processor with ThermalDef, thermal recipe requirements), `module_basic_radiator` (Radiator, cooling_capacity_w shared across thermal group) |
| `component_defs.json` | 1 component: `repair_kit` (50kg, 0.1 m³) |
| `pricing.json` | Import/export pricing: surcharges per kg, per-item base prices, importable/exportable flags |
| `scoring.json` | Run scoring config: 6 dimensions (id, name, weight, ceiling), 5 named thresholds (Startup→Space Magnate), computation_interval_ticks (default 24), scale_factor (default 2500). See Scoring section below. |
| `dev_base_state.json` | Pre-baked dev state: tick 0, 1 ship, 1 station with refinery module in inventory |

## Inventory & Refinery Design

**Inventory model:** Ships and stations carry `Vec<InventoryItem>` (not HashMap). Volume constraint: `inventory_volume_m3(items, content) ≤ capacity_m3`. Each item type computes volume differently (ore/slag/material by density, components by count, modules by def).

**Ore:** Mining produces `InventoryItem::Ore` with a `lot_id`, `asteroid_id`, `kg`, and snapshot of the asteroid's composition (deep-scanned if available, else true composition). Each asteroid produces distinct ore lots.

**Refinery:** Station modules with `ModuleBehaviorDef::Processor` tick at their defined interval. A processor: checks enabled + power + ore threshold → FIFO-consumes ore up to rate_kg → produces `Material` (element fraction × kg, quality from formula) + `Slag` (remainder). Materials of same element+quality merge. Slag merges into a single accumulating lot.

## Wear & Maintenance

**Wear model:** Each `ModuleState` has a `WearState { wear: f32 }` field (0.0–1.0). Processor modules accumulate `wear_per_run` after each processing run. Efficiency decreases in 3 bands defined by constants: nominal (1.0), degraded (0.75 at ≥0.5 wear), critical (0.5 at ≥0.8 wear). Modules auto-disable when wear reaches 1.0.

**Efficiency impact:** Wear reduces output quantities only — ore consumption stays constant. This creates economic pressure (wastes ore). Computed via `wear_efficiency(wear, constants)` pure function.

**Maintenance Bay:** `ModuleBehaviorDef::Maintenance` ticks at its `repair_interval_ticks`. Each run: finds most-worn module (highest wear, ID tiebreak), consumes `repair_kit_cost` RepairKits, reduces wear by `wear_reduction_per_run`. Skips if no worn modules or no kits. Re-enables auto-disabled modules when wear drops below 1.0.

**RepairKit:** `InventoryItem::Component { component_id: "repair_kit", count, quality }`. Station starts with 10. Craftable via Assembler (200kg Fe → 1 RepairKit, 360-tick interval). Stock capped at 50 by default.

**Events:** `WearAccumulated`, `ModuleAutoDisabled`, `MaintenanceRan`.

**Metrics:** `avg_module_wear`, `max_module_wear`, `repair_kits_remaining` (MetricsSnapshot v2).

## Assembler

**Assembler module:** `ModuleBehaviorDef::Assembler` ticks at `assembly_interval_ticks`. Each run: checks enabled + power + wear; matches recipe inputs against station inventory (Element filter by kg, Component filter by count); checks stock cap (`cap_override` takes priority over `max_stock` from def); if all inputs satisfied, not at cap, and output won't exceed station capacity, consumes inputs and produces output Component. Stalls if inputs missing or capacity insufficient (emits `ModuleStalled`/`ModuleResumed` on transition). Caps when output component count >= `max_stock` (emits `AssemblerCapped`/`AssemblerUncapped` on transition). `SetAssemblerCap` command overrides content cap at runtime. Wear applies via `wear_per_run`.

**Events:** `AssemblerRan`, `AssemblerCapped`, `AssemblerUncapped`.

**Metrics:** `assembler_active`, `assembler_stalled` (via `per_module_metrics` BTreeMap, MetricsSnapshot v11).

## Sensor Array

**Sensor Array module:** `ModuleBehaviorDef::SensorArray` ticks at `scan_interval_ticks`. Each run: checks enabled + power + wear; generates raw data of `data_kind` into the sim-wide `ResearchState.data_pool` using `generate_data()` with diminishing returns (keyed by `action_key`). This provides passive data generation for labs without requiring ship surveys.

**Events:** `DataGenerated { kind, amount }`.

## Storage Enforcement

**Storage enforcement:** Modules and ships respect station cargo capacity.
- **Processor stall:** Before running, a processor estimates its output volume. If the output would exceed the station's remaining capacity, the processor sets `stalled = true` and emits `ModuleStalled { station_id, module_id, shortfall_m3 }`. On the next tick where space is available, it clears the stall and emits `ModuleResumed { station_id, module_id }`. Stall events are emitted only on transition (not every tick).
- **Deposit blocking:** When a ship with a `Deposit` task arrives and there is not enough station capacity for its cargo, the task sets `blocked = true` and emits `DepositBlocked { ship_id, station_id, shortfall_m3 }`. If partial space is available, a partial deposit occurs (FIFO by inventory order). When full space opens, the remaining cargo is deposited and `DepositUnblocked { ship_id, station_id }` is emitted.
- **Metric:** `processor_stalled` (via `per_module_metrics`) — number of processor modules currently in `stalled = true` state.

## Economy & Trade

**Balance:** `GameState.balance` (f64) starts at $1,000,000,000. Funds are deducted on import and credited on export.

**PricingTable:** Loaded from `content/pricing.json`. Contains `import_surcharge_per_kg` and `export_surcharge_per_kg` (flat surcharges added per kg of traded goods), plus `items: HashMap<String, PricingEntry>` keyed by item identifier (element ID, component ID, or module def ID). Each `PricingEntry` has `base_price_per_unit`, `importable: bool`, `exportable: bool`.

**TradeItemSpec:** Specifies what to trade. Three variants:
- `Material { element, kg }` — bulk material by element and mass
- `Component { component_id, count }` — components by ID and quantity
- `Module { module_def_id }` — a station module by definition ID

**Import cost:** `base_price_per_unit * quantity + import_surcharge_per_kg * total_mass_kg`. Deducted from balance. Items added to station inventory.

**Export revenue:** `base_price_per_unit * quantity - export_surcharge_per_kg * total_mass_kg`. Credited to balance. Items removed from station inventory.

**Commands:** `Command::Import { station_id, item_spec }` and `Command::Export { station_id, item_spec }`. Processed during tick step 1 (apply_commands). Emits `InsufficientFunds` if balance is too low for an import.

**Events:**
- `ItemImported { station_id, item_spec, cost, balance_after }` — successful import
- `ItemExported { station_id, item_spec, revenue, balance_after }` — successful export
- `ShipConstructed { station_id, ship_id }` — shipyard assembler produced a new ship
- `InsufficientFunds { station_id, action, required, available }` — import rejected due to low balance
- `ModuleAwaitingTech { station_id, module_id, tech_id }` — module skipped because required tech is not yet unlocked

**OutputSpec::Ship:** Assembler recipe output variant `Ship { cargo_capacity_m3 }`. When a shipyard assembler completes a recipe with this output, a new `ShipState` is created at the station's location node with the specified cargo capacity. Requires `tech_ship_construction` to be unlocked; otherwise emits `ModuleAwaitingTech` and skips.

**Autopilot thruster import:** The `AutopilotController` auto-imports thrusters when conditions are met: station has an enabled shipyard module, `tech_ship_construction` is unlocked, station has >= 5000 kg Fe, station has < 4 thrusters in inventory, and balance > 2x the import cost. Imports up to 4 thrusters total.

**API endpoints:**
- `POST /api/v1/command` — enqueue a `Command` (JSON body) into the daemon's command queue, processed next tick
- `GET /api/v1/pricing` — returns the `PricingTable` as JSON
- `GET /api/v1/content` — returns tech definitions, lab rates (points/hr), data pool net rates (per kind/hr), `minutes_per_tick`, and recipe catalog (`Record<RecipeId, RecipeDef>`)
- `GET /api/v1/perf` — per-step tick timing stats (mean/p50/p95/max µs) from rolling buffer of last 1,000 ticks. Requires `instrumentation` feature or debug build.

**Future direction (not yet built):**
- Ore keyed by composition hash instead of asteroid ID — compatible ores blend naturally.
- Blending tolerance as a tech unlock: ±2% basic, ±10% advanced.
- Volatiles flag ore as unblendable.
- Component output from processors (type defined but no-op).
- Storage modules (type defined but tick loop skips them).

## Thermal System

**Thermal types:**

| Type | Purpose |
|---|---|
| `ThermalState` | Per-module runtime thermal state: `temp_mk: u32` (milli-Kelvin), `thermal_group: Option<ThermalGroupId>`, `overheat_zone: OverheatZone`, `overheat_disabled: bool` |
| `ThermalDef` | Content-driven module thermal properties: `heat_capacity_j_per_k: f32`, `passive_cooling_coefficient: f32`, `max_temp_mk: u32`, `operating_min_mk: Option<u32>`, `operating_max_mk: Option<u32>`, `thermal_group: Option<ThermalGroupId>`, `idle_heat_generation_w: Option<f32>` |
| `MaterialThermalProps` | Thermal properties on a `Material` inventory item: `temp_mk: u32`, `phase: Phase`, `latent_heat_buffer_j: i64` |
| `Phase` | Enum: `Solid`, `Liquid` |
| `OverheatZone` | Enum: `Nominal` (default), `Warning`, `Critical` |
| `RecipeThermalReq` | Per-recipe thermal requirements: `min_temp_mk`, `optimal_min_mk`, `optimal_max_mk`, `max_temp_mk`, `heat_per_run_j` |
| `RadiatorDef` | Radiator module behavior: `cooling_capacity_w: f32` |

**Thermal tick step (3.6):** Runs after maintenance, before research. For each station, groups modules by `thermal_group` (BTreeMap for deterministic order, modules sorted by ID within each group). Three passes:

1. **Idle heat generation:** For each enabled thermal module with `idle_heat_generation_w`, applies `Q = idle_heat_generation_w * dt_s` as heat. This lets thermal modules (e.g. smelters) preheat from ambient temperature without needing recipe inputs. Modules start at ambient (293K) and warm up over time.
2. **Passive cooling:** For each thermal module, applies Newton's cooling law toward the sink temperature: `Q_loss = passive_cooling_coefficient * dt_s * (T - T_sink) / 1000`. Converts energy to temperature delta via `heat_capacity_j_per_k`. Temperature clamped to `[sink_temp, 10_000_000 mK]`.
3. **Radiator cooling:** Per group, sums total radiator `cooling_capacity_w` (adjusted for wear efficiency), distributes cooling energy evenly across all thermal modules in the group. Same energy-to-delta conversion and clamping.

After temperature updates, checks all thermal modules for overheat zone transitions and emits events.

**Smelter module:** A Processor with a `ThermalDef`. Recipes have a `RecipeThermalReq` specifying thermal requirements:
- Below `min_temp_mk`: processor stalls (emits `ProcessorTooCold`).
- `min_temp_mk` to `optimal_min_mk`: efficiency ramps 80% to 100% (reduced yield).
- `optimal_min_mk` to `optimal_max_mk`: 100% efficiency, 100% quality.
- `optimal_max_mk` to `max_temp_mk`: quality degrades 100% to 60%.
- Above `max_temp_mk`: quality drops to 30%.
- Each processing run generates `heat_per_run_j` (positive = exothermic, negative = endothermic).

**Radiator module:** `ModuleBehaviorDef::Radiator`. Provides passive cooling via `cooling_capacity_w`, shared across all thermal modules in the same `thermal_group`. Subject to wear efficiency. No operating temperature requirement.

**Overheat escalation:**

| Zone | Trigger | Effect |
|---|---|---|
| Nominal | Below `max_temp_mk` + warning offset | Normal operation (1x wear) |
| Warning | `max_temp_mk` + 200K (`thermal_overheat_warning_offset_mk`) | 2x wear (`thermal_wear_multiplier_warning`) |
| Critical | `max_temp_mk` + 500K (`thermal_overheat_critical_offset_mk`) | 4x wear (`thermal_wear_multiplier_critical`), module auto-disabled |
| Damage | `max_temp_mk` + 800K (`thermal_overheat_damage_offset_mk`) | Wear jumps to 0.8, module auto-disabled. Recoverable with maintenance. |

Zone transitions emit `OverheatWarning`, `OverheatCritical`, `OverheatDamage`, or `OverheatCleared` events.

**Thermal constants (in `Constants`):**

| Constant | Default | Purpose |
|---|---|---|
| `thermal_sink_temp_mk` | 293,000 (20 C) | Ambient/radiator sink temperature |
| `thermal_overheat_warning_offset_mk` | 200,000 | Offset above `max_temp_mk` for Warning zone |
| `thermal_overheat_critical_offset_mk` | 500,000 | Offset above `max_temp_mk` for Critical zone |
| `thermal_wear_multiplier_warning` | 2.0 | Wear rate multiplier in Warning zone |
| `thermal_wear_multiplier_critical` | 4.0 | Wear rate multiplier in Critical zone |

**Module initialization:** Thermal modules start at `operating_min_mk` (if set) or ambient (`DEFAULT_AMBIENT_TEMP_MK` = 293,000 mK / 20 C). `MaterialThermalProps` defaults to ambient, Solid phase, zero latent heat buffer.

**Events:** `ProcessorTooCold`, `OverheatWarning`, `OverheatCritical`.

**Metrics (MetricsSnapshot):** `station_max_temp_mk`, `station_avg_temp_mk`, `overheat_warning_count`, `overheat_critical_count`, `heat_wear_multiplier_avg`.

## Slag Jettison

**Command:** `JettisonSlag { station_id }` — removes all `InventoryItem::Slag` from the station's inventory. Emits `SlagJettisoned { station_id, kg }` with the total mass jettisoned. No event if no slag is present.

**Autopilot:** Auto-jettisons when `inventory_volume_m3(station) / cargo_capacity_m3 >= constants.autopilot_slag_jettison_pct` (default 0.75). Set to 1.0+ to disable. Checked each tick after station module and lab assignment commands.

## Benchmark Runner (sim_bench)

Automated scenario runner for testing simulation behavior across multiple seeds. Runs seeds in parallel with rayon, computes cross-seed summary statistics.

**Scenario file format** (JSON):

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | required | Scenario name (used in output directory) |
| `ticks` | u64 | required | Number of ticks to simulate per seed |
| `metrics_every` | u64 | `60` | Metrics snapshot interval (ticks) |
| `seeds` | list or range | required | `[1, 2, 3]` or `{"range": [1, 100]}` |
| `content_dir` | string | `"./content"` | Path to content directory |
| `overrides` | object | `{}` | Constants overrides (key → value) |

**Override keys:** All fields on `Constants` struct — `survey_scan_minutes`, `deep_scan_minutes`, `survey_tag_detection_probability`, `asteroid_count_per_template`, `asteroid_mass_min_kg`, `asteroid_mass_max_kg`, `ship_cargo_capacity_m3`, `station_cargo_capacity_m3`, `mining_rate_kg_per_minute`, `deposit_minutes`, `station_power_available_per_minute`, `autopilot_volatile_threshold_kg`, `autopilot_refinery_threshold_kg`, `autopilot_slag_jettison_pct`, `autopilot_export_batch_size_kg`, `autopilot_export_min_revenue`, `autopilot_lh2_threshold_kg`, `autopilot_budget_cap_fraction`, `autopilot_lh2_abundant_multiplier`, `data_generation_peak`, `data_generation_floor`, `data_generation_decay_rate`, `wear_band_degraded_threshold`, `wear_band_critical_threshold`, `wear_band_degraded_efficiency`, `wear_band_critical_efficiency`, `minutes_per_tick`. Module overrides: `module.<type>.<field>`. Per-element/per-tag autopilot settings (confidence thresholds, export reserves) are now in `content/autopilot.json`.

**Output structure:**

```
runs/<name>_<timestamp>/
  scenario.json          # Copy of input scenario
  summary.json           # Cross-seed summary statistics
  seed_1/
    run_info.json
    metrics_000.csv
  seed_2/
    ...
```

**Summary metrics:** `storage_saturation_pct`, `fleet_idle_pct`, `processor_starved`, `techs_unlocked`, `avg_module_wear`, `repair_kits_remaining`, `export_revenue_total`, `export_count`. Each reports mean, min, max, stddev across seeds.

**Collapse detection:** A seed is "collapsed" if the final snapshot has `processor_starved > 0` AND `fleet_idle == fleet_total`.

**Example scenario:** `scenarios/cargo_sweep.json` — 5 seeds × 10k ticks with storage capacity and wear threshold overrides.

## MVP Scope

- **MVP-0 (done):** sim_core tick + tests, sim_control autopilot, sim_cli run loop.
- **MVP-1 (done):** sim_daemon HTTP server, SSE event stream, React mission control UI.
- **MVP-2 (done):** Mining loop — cargo holds, Mine/Deposit tasks, ore extraction.
- **MVP-3 (done):** Refinery system — inventory model, station modules, processor tick logic.
- **FE Foundations (done):** Nav sidebar, sortable tables, color refresh.
- **Solar System Map (done):** SVG orbital map, d3-zoom, entity markers, tooltips, detail cards.
- **Smooth Streaming (done):** useAnimatedTick, 200ms heartbeat, measured tick rate.
- **Procedural Sites (done):** Scan site replenishment with deterministic UUIDs.
- **Wear & Maintenance Phase 1 (done):** Module wear accumulation, 3-band efficiency, auto-disable, Maintenance Bay, RepairKit, wear metrics.
- **Storage Enforcement (done):** Processors stall when output exceeds capacity; ships wait when deposit blocked; partial deposits.
- **Alerts (done):** Pure-Rust AlertEngine with 9 rules, evaluates after each metrics sample, SSE events, UI badges.
- **Assembler (done):** Basic assembler module (200kg Fe → 1 RepairKit, 360-tick interval), recipe system, stall/resume logic, stock cap (max_stock), wear.
- **Fleet Panel Expandable Rows (done):** Clickable fleet rows expand to show detail sections.
- **Draggable Panels (done):** @dnd-kit panel reordering, persisted to localStorage.
- **Pause/Resume (done):** AtomicBool tick loop pause, POST pause/resume endpoints, StatusBar toggle, spacebar shortcut.
- **Keyboard Shortcuts (done):** Spacebar (pause/resume), Cmd/Ctrl+S (save).
- **Sound Effects (done):** Web Audio synthesis (`sounds.ts`) — noise-burst click for pause/resume, two-tone beep for save.
- **Pause Tick Freeze (done):** `useAnimatedTick` freezes `displayTick` immediately when paused (no drift).
- **Benchmark Runner (done):** `sim_bench` crate — JSON scenario files, constant overrides, parallel seed execution (rayon), per-seed CSV metrics, cross-seed summary statistics, collapse detection.
- **Economy & Trade (done):** Balance system ($1B start), import/export commands, pricing table from pricing.json, shipyard assembler (OutputSpec::Ship), thruster import autopilot, Economy UI panel, daemon command queue + pricing endpoint.
- **Heat System MVP (done):** Thermal state on modules (milli-Kelvin), smelter (processor with thermal requirements), radiator (shared cooling per thermal group), overheat escalation (Warning 2x wear, Critical 4x wear + auto-disable), thermal metrics, thermal alerts, UI temperature readouts and badges.

## Scoring

**Run scoring** evaluates simulation performance across 6 weighted dimensions, producing a composite score and named threshold.

**Content schema** (`content/scoring.json`):
- `dimensions[]` — Array of `DimensionDef`: `id` (string), `name` (string), `weight` (f64, all must sum to 1.0), `ceiling` (f64, normalization ceiling)
- `thresholds[]` — Array of `ThresholdDef`: `name` (string), `min_score` (f64, ascending order)
- `computation_interval_ticks` — How often score is recomputed (default: 24)
- `scale_factor` — Multiplier on weighted sum to produce composite (default: 2500.0)

**Dimensions** (6):

| Dimension | Weight | Inputs |
|---|---|---|
| Industrial Output | 25% | total_material_kg/tick, assembler active count |
| Research Progress | 20% | techs_unlocked/total, total_scan_data growth |
| Economic Health | 20% | balance trend, export_revenue_total growth |
| Fleet Operations | 15% | fleet utilization, ships constructed |
| Efficiency | 10% | inverted avg_module_wear, power util, storage util |
| Expansion | 10% | station count, zone activity, fleet size |

**Named thresholds:** Startup (0) → Contractor (200) → Enterprise (500) → Industrial Giant (1000) → Space Magnate (2000+)

**Types** (`sim_core::scoring`):
- `ScoringConfig` — content-loaded config (dimensions, thresholds, interval, scale_factor)
- `RunScore` — output: per-dimension `BTreeMap<String, DimensionScore>`, composite (f64), threshold (String), tick (u64)
- `DimensionScore` — per-dimension: id, name, raw_value, normalized [0.0–1.0], weighted contribution
- `validate_scoring_config()` — validates weights sum, ascending thresholds, positive ceilings
