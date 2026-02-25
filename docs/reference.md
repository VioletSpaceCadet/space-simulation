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
| `ModuleState` | Installed module: `id`, `def_id`, `enabled`, `kind_state` (Processor, Storage, Maintenance, or Assembler), `wear: WearState`. Processor/Assembler have `stalled: bool`. Assembler also has `capped: bool`, `cap_override: HashMap<ComponentId, u32>` |
| `WearState` | `wear: f32` (0.0–1.0). Embedded on any wearable entity. |
| `TaskKind` | `Idle`, `Survey`, `DeepScan`, `Mine { asteroid, duration_ticks }`, `Deposit { station, blocked }`, `Transit { destination, total_ticks, then }` |
| `Command` | `AssignShipTask`, `InstallModule`, `UninstallModule`, `SetModuleEnabled`, `SetModuleThreshold`, `AssignLabTech`, `SetAssemblerCap`, `JettisonSlag` |
| `GameContent` | Static config: techs, solar system, asteroid templates, elements, module_defs, component_defs, constants |
| `ModuleDef` | Module definition with `ModuleBehaviorDef` (Processor, Storage, Maintenance, Assembler, Lab, or SensorArray), `wear_per_run` |
| `ComponentDef` | Component definition: `id`, `name`, `mass_kg`, `volume_m3` |
| `MaintenanceDef` | Maintenance module behavior: `repair_interval_ticks`, `wear_reduction_per_run`, `repair_kit_cost` |
| `AssemblerDef` | Assembler module behavior: `assembly_interval_ticks`, `recipes` (list of input filters + output component), `max_stock: HashMap<ComponentId, u32>` (optional stock cap per output component) |
| `TechEffect` | `EnableDeepScan` or `DeepScanCompositionNoise { sigma }` |
| `ResearchDomain` | Enum: `Materials`, `Exploration`, `Engineering` — categorises techs and lab output |
| `DomainProgress` | Per-tech domain point tracking: `{ materials: f64, exploration: f64, engineering: f64 }` |
| `DataKind` | Enum: `ScanData`, `MiningData`, `EngineeringData` — type of raw data a lab consumes |
| `LabDef` | Lab module behavior definition: `data_kind`, `domain`, `throughput_per_tick` |
| `LabState` | Lab runtime state (embedded in `ModuleState::kind_state`): `assigned_tech: Option<TechId>` |

Note: `FacilitiesState` has been removed. Research state is fully contained in `ResearchState`.

## Research System

**Lab-based domain model:** Labs are station modules (`ModuleBehaviorDef::Lab`) that consume raw data from the sim-wide `ResearchState.data_pool` each tick and produce domain-specific research points toward an assigned tech.

**`ResearchState` fields:** `unlocked: HashSet<TechId>`, `data_pool: HashMap<DataKind, f64>`, `domain_progress: HashMap<TechId, DomainProgress>`.

**Raw data generation:** Ships generate raw data as a side-effect of tasks. `data_yield(task, content)` computes yield for a completed task. `generate_data(state, task, content)` applies the yield to `ResearchState.data_pool`. Data is sim-wide — not stored in ship or station inventory.

**Research roll:** Occurs every `research_roll_interval_ticks` ticks (not every tick). For each tech with an assigned lab:
1. Lab consumes data from `data_pool` (up to `throughput_per_tick × interval`).
2. Domain points accumulate in `DomainProgress` for that tech.
3. Unlock probability: `p = 1 - e^(-effective / difficulty)` where `effective = sufficiency × total_points` and `sufficiency = geometric_mean(per-domain ratios)`.
4. If roll succeeds, tech is added to `ResearchState.unlocked`.

**Domain sufficiency:** `geometric_mean` of the per-domain ratios (actual / required). Techs that require only one domain have sufficiency = that domain's ratio (clamped to 1.0 max).

**`geometric_mean(values: &[f64]) -> f64`:** Product of values raised to `1/n`. Returns 0.0 for empty slice.

**Constants (in `constants.json`):**

| Constant | Purpose |
|---|---|
| `research_roll_interval_ticks` | How many ticks between research unlock rolls |
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
| `module_defs.json` | 3 modules: `module_basic_iron_refinery` (Processor, 60-tick interval, wear_per_run=0.01), `module_maintenance_bay` (Maintenance, 30-tick interval, reduces 0.2 wear, costs 1 RepairKit), `module_basic_assembler` (Assembler, 360-tick interval, wear_per_run=0.008, 200kg Fe → 1 RepairKit, max_stock: repair_kit=50) |
| `component_defs.json` | 1 component: `repair_kit` (50kg, 0.1 m³) |
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

**Metrics:** `assembler_active_count`, `assembler_stalled_count` (MetricsSnapshot v3).

## Sensor Array

**Sensor Array module:** `ModuleBehaviorDef::SensorArray` ticks at `scan_interval_ticks`. Each run: checks enabled + power + wear; generates raw data of `data_kind` into the sim-wide `ResearchState.data_pool` using `generate_data()` with diminishing returns (keyed by `action_key`). This provides passive data generation for labs without requiring ship surveys.

**Events:** `DataGenerated { kind, amount }`.

## Storage Enforcement

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

**Override keys:** All fields on `Constants` struct — `survey_scan_ticks`, `deep_scan_ticks`, `travel_ticks_per_hop`, `survey_tag_detection_probability`, `asteroid_count_per_template`, `asteroid_mass_min_kg`, `asteroid_mass_max_kg`, `ship_cargo_capacity_m3`, `station_cargo_capacity_m3`, `mining_rate_kg_per_tick`, `deposit_ticks`, `station_power_available_per_tick`, `autopilot_iron_rich_confidence_threshold`, `autopilot_refinery_threshold_kg`, `autopilot_slag_jettison_pct`, `research_roll_interval_ticks`, `data_generation_peak`, `data_generation_floor`, `data_generation_decay_rate`, `wear_band_degraded_threshold`, `wear_band_critical_threshold`, `wear_band_degraded_efficiency`, `wear_band_critical_efficiency`.

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

**Summary metrics:** `storage_saturation_pct`, `fleet_idle_pct`, `refinery_starved_count`, `techs_unlocked`, `avg_module_wear`, `repair_kits_remaining`. Each reports mean, min, max, stddev across seeds.

**Collapse detection:** A seed is "collapsed" if the final snapshot has `refinery_starved_count > 0` AND `fleet_idle == fleet_total`.

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
